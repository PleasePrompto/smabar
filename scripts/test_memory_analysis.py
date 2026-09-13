"""Offline allocation analyses must preserve record boundaries and evidence limits."""

import importlib.util
import io
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest.mock import patch


def load_analysis(name):
    path = Path(__file__).parent / "memory" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


phase = load_analysis("heaptrack-phase")
retained = load_analysis("heaptrack-retained")
pas = load_analysis("pas-status-parse")


class HeaptrackAnalysisTests(unittest.TestCase):
    def test_original_self_checks(self):
        phase.self_check()
        retained.self_check()

    def test_snapshot_refuses_existing_output_and_missing_record_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "live.raw"
            destination = Path(directory) / "warm.raw"
            source.write_bytes(b"v 10500 3\n")
            destination.write_bytes(b"previous measurement")
            with self.assertRaises(FileExistsError):
                phase.snapshot(source, destination)
            self.assertEqual(destination.read_bytes(), b"previous measurement")
            source.write_bytes(b"partial")
            with self.assertRaisesRegex(ValueError, "No complete record"):
                phase.snapshot(source, destination)

    def test_pointer_reuse_audits_are_separate_from_retained_deltas(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "end.raw"
            warm = b"v 10500 3\nt 1000 0\n+ 20 1 123\n"
            end = warm + b"+ 30 1 123\n- 999\n- 123\n+ 10 1 456\n"
            source.write_bytes(end)
            result = retained.retained(source, {"warm": len(warm), "end": len(end)})
            self.assertEqual(result["phases"]["end"], {"bytes": 16, "blocks": 1})
            self.assertEqual(result["deltas"]["end"][0]["bytesDelta"], -16)
            self.assertEqual(
                result["eventAuditIntervals"],
                [
                    {
                        "from": "warm",
                        "to": "end",
                        "allocationEvents": 2,
                        "freeEvents": 2,
                        "unmatchedFrees": 1,
                        "duplicateLivePointers": 1,
                        "overwrittenTrackedBytes": 32,
                    }
                ],
            )

    def test_unchanged_phases_and_invalid_checkpoint_order(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "end.raw"
            data = b"v 10500 3\nt 1000 0\n+ 20 1 123\n"
            source.write_bytes(data)
            result = retained.retained(source, {"warm": len(data), "idle": len(data)})
            self.assertEqual(result["phases"]["warm"], result["phases"]["idle"])
            self.assertEqual(result["deltas"]["idle"], [])
            with self.assertRaisesRegex(ValueError, "trace order"):
                retained.retained(source, {"later": len(data), "earlier": 10})
            with self.assertRaisesRegex(ValueError, "record boundary"):
                retained.retained(source, {"warm": len(data) - 1, "end": len(data)})
            source.write_bytes(data + b"+ 20")
            with self.assertRaisesRegex(ValueError, "partial record"):
                retained.retained(source, {"warm": len(data), "end": len(data) + 4})

    def test_corrupt_trace_references_fail_before_stack_reconstruction(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "end.raw"
            header = b"v 10500 3\n"
            for invalid in (b"t 1000 1\n", b"t 1000 9\n", b"+ 20 1 123\n"):
                with self.subTest(record=invalid):
                    source.write_bytes(header + invalid)
                    with self.assertRaisesRegex(
                        ValueError, "Trace parent|unknown trace"
                    ):
                        retained.retained(
                            source, {"warm": len(header), "end": source.stat().st_size}
                        )

    def test_live_source_is_read_only_to_last_requested_checkpoint(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "live.raw"
            warm = b"v 10500 3\nt 1000 0\n"
            end = warm + b"+ 20 1 123\n"
            source.write_bytes(end + b"+ incomplete later record")
            result = retained.retained(source, {"warm": len(warm), "end": len(end)})
            self.assertEqual(result["phases"]["end"], {"bytes": 32, "blocks": 1})


def pas_report(pid=123, allocated=32):
    summary = f"Alloc: {allocated}/64 (CO)/80 (CT)/128 (R); Frag: 48 (60%); Cached: 8"
    return (
        f"{pid}: Heap Status:\n"
        "    Heap 0x10:\n"
        "        Type = Common Primitive\n"
        f"        Segregated Heap 0x20: {summary}\n"
        f"        Total Summary: {summary}\n"
        f"    All Heaps Summary: {summary}\n"
        "    Num Heaps: 1\n"
        "    Physical Page Sharing Pool Balance: 0\n"
    ).encode()


class PasAnalysisTests(unittest.TestCase):
    def test_pid_filter_incomplete_reports_and_nonoverlapping_totals(self):
        complete = pas_report()
        data = pas_report(pid=999) + complete + pas_report()[:-1]
        records, rejected = pas.parse_records(data, 123)
        self.assertEqual(len(records), 1)
        self.assertEqual(len(rejected), 1)
        heap = records[0]["heaps"]["Common Primitive"]
        self.assertEqual(heap["total"]["allocated"], 32)
        self.assertEqual(heap["components"]["Segregated Heap"]["allocated"], 32)
        self.assertEqual(heap["total"]["freeCommittedObjects"], 32)
        self.assertEqual(heap["total"]["committedOutsideObjects"], 16)
        self.assertEqual(heap["total"]["decommitted"], 48)
        self.assertEqual(records[0]["endByte"] - records[0]["startByte"], len(complete))

    def test_impossible_summary_fails_visibly(self):
        with self.assertRaisesRegex(ValueError, "Inconsistent PAS summary"):
            pas.parse_records(pas_report(allocated=100), 123)

    def test_alignment_preserves_unknown_early_time_and_selects_nearest_sample(self):
        records = [
            {"startByte": 5, "endByte": 20},
            {"startByte": 40, "endByte": 60},
            {"startByte": 75, "endByte": 90},
        ]
        offsets = [
            {"bytes": 30, "monotonic_seconds": 1},
            {"bytes": 40, "monotonic_seconds": 5},
            {"bytes": 70, "monotonic_seconds": 6},
        ]
        samples = [
            {"cycle": 393, "monotonic_seconds": 5.4},
            {"cycle": 394, "monotonic_seconds": 5.9},
        ]
        pas.align_records(records, offsets, samples)
        self.assertIsNone(records[0]["timeBounds"])
        self.assertEqual(records[1]["timeBounds"], {"lower": 5, "upper": 6})
        self.assertEqual(records[1]["nearestPhase"]["cycle"], 393)
        self.assertIsNone(records[2]["timeBounds"])
        with self.assertRaisesRegex(ValueError, "must not decrease"):
            pas.align_records(records, offsets, list(reversed(samples)))
        with self.assertRaisesRegex(ValueError, "must both contain rows"):
            pas.align_records(records, offsets, [])

    def test_phase_deltas_use_preceding_report_and_report_its_actual_age(self):
        records, _ = pas.parse_records(
            pas_report(allocated=32) + pas_report(allocated=48), 123
        )
        for record, clock in zip(records, (10, 20)):
            record["timeBounds"] = {"lower": clock - 1, "upper": clock}
            record["nearestPhase"] = {"cycle": clock * 10 - 7}
        result = pas.phase_comparisons(
            records,
            [
                {"cycle": 400, "monotonic_seconds": 19},
                {"cycle": 600, "monotonic_seconds": 21},
            ],
        )
        self.assertEqual(result["checkpoints"][0]["recordIndex"], 0)
        self.assertEqual(result["checkpoints"][0]["maximumAgeSeconds"], 10)
        delta = result["deltas"][0]
        self.assertEqual((delta["from"], delta["to"]), ("cycle-400", "cycle-600"))
        self.assertEqual(delta["heaps"]["Common Primitive"]["allocated"], 16)

    def test_cli_requires_paired_timestamps_before_reading_log(self):
        with (
            patch(
                "sys.argv",
                [
                    "pas-status-parse.py",
                    "missing.log",
                    "--pid",
                    "123",
                    "--offsets",
                    "offsets.jsonl",
                ],
            ),
            redirect_stderr(io.StringIO()) as errors,
        ):
            with self.assertRaises(SystemExit) as error:
                pas.main()
            self.assertEqual(error.exception.code, 2)
            self.assertIn("must be supplied together", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
