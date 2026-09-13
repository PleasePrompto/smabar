"""The memory sampler must include descendants without double-counting RSS."""

import importlib.util
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "memory", Path(__file__).with_name("measure-memory.py")
)
assert spec is not None and spec.loader is not None
memory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(memory)


class MemoryTests(unittest.TestCase):
    def test_tree_totals_and_disappearing_children(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            for pid, parent, name in [
                (10, 1, "smabar"),
                (20, 10, "uv"),
                (30, 20, "python3"),
                (40, 1, "unrelated"),
            ]:
                target = proc / str(pid)
                target.mkdir()
                (target / "stat").write_text(
                    f"{pid} ({name}) S {parent} " + "0 " * 17 + "123\n"
                )
                (target / "smaps_rollup").write_text(
                    "".join(
                        f"{key}: {value} kB\n"
                        for key, value in zip(memory.FIELDS, [10, 50, 2, 5, 6])
                    )
                )
            (proc / "20" / "smaps_rollup").unlink()
            result = memory.sample(10, proc)
            self.assertEqual([row["pid"] for row in result["processes"]], [10, 30])
            self.assertEqual(
                [row["start_ticks"] for row in result["processes"]], ["123", "123"]
            )
            self.assertEqual(result["total"]["Pss"], 20)
            self.assertEqual(result["total"]["Rss"], 100)
            self.assertEqual(result["exited_during_sample"], [20])
            self.assertEqual(result["groups"]["python"]["Private_Dirty"], 5)
            with self.assertRaises(ProcessLookupError):
                memory.sample(99, proc)

    def test_root_exit_or_reuse_aborts_the_sample(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            target = proc / "10"
            target.mkdir()
            (target / "stat").write_text("10 (smabar) S 1 " + "0 " * 17 + "124\n")
            with patch.object(
                memory, "process_tree", return_value={10: (1, "smabar", "123")}
            ):
                with self.assertRaisesRegex(ProcessLookupError, "main process"):
                    memory.sample(10, proc)
                (target / "smaps_rollup").write_text("")
                with self.assertRaisesRegex(ProcessLookupError, "main PID was reused"):
                    memory.sample(10, proc)

    def test_invalid_interval_is_rejected_before_sampling(self):
        for interval in ("nan", "inf", "0"):
            with (
                self.subTest(interval=interval),
                patch("sys.argv", ["measure-memory.py", "1", "--interval", interval]),
                patch.object(
                    memory, "sample", return_value={"root_start_ticks": "123"}
                ) as sample,
                redirect_stdout(io.StringIO()),
                redirect_stderr(io.StringIO()),
            ):
                with self.assertRaises(SystemExit) as error:
                    memory.main()
                self.assertEqual(error.exception.code, 2)
                sample.assert_not_called()
