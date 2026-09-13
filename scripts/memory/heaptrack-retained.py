"""Count retained bytes and blocks at complete-record checkpoints in one raw trace."""

import argparse
import json
import tempfile
from collections import defaultdict
from itertools import pairwise
from pathlib import Path


def retained(source: Path, checkpoints: dict[str, int]) -> dict[str, object]:
    """Read only through the final checkpoint, even if the source keeps growing."""
    if len(checkpoints) < 2 or any(offset <= 0 for offset in checkpoints.values()):
        raise ValueError("Provide at least two positive byte checkpoints")
    if any(left > right for left, right in pairwise(checkpoints.values())):
        raise ValueError("Checkpoints must follow trace order")
    live: dict[int, tuple[int, int]] = {}
    traces = [(0, 0)]
    phases = {}
    event_audits = {}
    audit = {
        "allocationEvents": 0,
        "freeEvents": 0,
        "unmatchedFrees": 0,
        "duplicateLivePointers": 0,
        "overwrittenTrackedBytes": 0,
    }
    position = 0
    offsets: dict[int, list[str]] = defaultdict(list)
    for label, offset in checkpoints.items():
        offsets[offset].append(label)
    with source.open("rb") as stream:
        for line in stream:
            position += len(line)
            if not line.endswith(b"\n"):
                raise ValueError(
                    "Trace ends in a partial record; create a snapshot with heaptrack-phase.py"
                )
            parts = line.split()
            if not parts:
                raise ValueError(f"Empty trace record at byte {position}")
            if parts[0] == b"+":
                size, trace, pointer = (int(part, 16) for part in parts[1:])
                if not 0 <= trace < len(traces):
                    raise ValueError(
                        f"Allocation refers to unknown trace {trace} at byte {position}"
                    )
                audit["allocationEvents"] += 1
                if previous := live.get(pointer):
                    audit["duplicateLivePointers"] += 1
                    audit["overwrittenTrackedBytes"] += previous[0]
                live[pointer] = (size, trace)
            elif parts[0] == b"-":
                audit["freeEvents"] += 1
                if live.pop(int(parts[1], 16), None) is None:
                    audit["unmatchedFrees"] += 1
            elif parts[0] == b"t":
                address, parent = int(parts[1], 16), int(parts[2], 16)
                if not 0 <= parent < len(traces):
                    raise ValueError(
                        f"Trace parent {parent} must precede its child at byte {position}"
                    )
                traces.append((address, parent))
            if position in offsets:
                summary: dict[int, list[int]] = defaultdict(lambda: [0, 0])
                for size, trace in live.values():
                    summary[trace][0] += size
                    summary[trace][1] += 1
                for label in offsets[position]:
                    phases[label] = dict(summary)
                    event_audits[label] = audit.copy()
                if position == max(offsets):
                    break
    if len(phases) != len(checkpoints):
        raise ValueError("Checkpoint is outside trace or not on a record boundary")
    labels = list(checkpoints)
    baseline = phases[labels[0]]
    comparisons = {}
    for label in labels[1:]:
        end = phases[label]
        rows = []
        for trace in baseline.keys() | end.keys():
            before_bytes, before_count = baseline.get(trace, [0, 0])
            after_bytes, after_count = end.get(trace, [0, 0])
            if after_bytes == before_bytes and after_count == before_count:
                continue
            stack = []
            frame = trace
            while frame:
                address, frame = traces[frame]
                stack.append(hex(address))
            rows.append(
                {
                    "trace": trace,
                    "bytesDelta": after_bytes - before_bytes,
                    "blocksDelta": after_count - before_count,
                    "stack": stack,
                }
            )
        comparisons[label] = sorted(
            rows, key=lambda row: row["bytesDelta"], reverse=True
        )
    return {
        "phases": {
            label: {
                "bytes": sum(v[0] for v in summary.values()),
                "blocks": sum(v[1] for v in summary.values()),
            }
            for label, summary in phases.items()
        },
        "deltas": comparisons,
        "eventAudits": event_audits,
        "eventAuditIntervals": [
            {
                "from": before,
                "to": after,
                **{
                    key: event_audits[after][key] - event_audits[before][key]
                    for key in audit
                },
            }
            for before, after in pairwise(labels)
        ],
    }


def self_check() -> None:
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / "trace.raw"
        warm = b"v 10500 3\nt 1000 0\n+ 20 1 123\n"
        source.write_bytes(warm + b"- 123\n+ 30 1 456\n- 999\n")
        result = retained(source, {"warm": len(warm), "end": source.stat().st_size})
        assert result["phases"]["warm"] == {"bytes": 32, "blocks": 1}
        assert result["deltas"]["end"][0] == {
            "trace": 1,
            "bytesDelta": 16,
            "blocksDelta": 0,
            "stack": ["0x1000"],
        }
        assert result["eventAudits"]["end"]["unmatchedFrees"] == 1
        assert result["eventAudits"]["end"]["duplicateLivePointers"] == 0
        source.write_bytes(warm + b"+ 30 1 123\n")
        overwritten = retained(
            source, {"warm": len(warm), "end": source.stat().st_size}
        )
        assert overwritten["eventAudits"]["end"]["duplicateLivePointers"] == 1
        assert overwritten["eventAudits"]["end"]["overwrittenTrackedBytes"] == 32


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog="Phases must be prefixes of the same uncompressed trace, in time order. "
        "Only the last file is parsed; earlier file sizes supply checkpoints. "
        "Nonzero unmatchedFrees or duplicateLivePointers limit the reliability "
        "of retained-byte totals.",
    )
    parser.add_argument(
        "phases",
        type=Path,
        nargs="*",
        help="baseline then later snapshots, with distinct filename stems",
    )
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="run the synthetic allocation/free/audit checks",
    )
    args = parser.parse_args()
    if args.self_check:
        self_check()
        print("heaptrack retained-allocation check passed")
    elif len(args.phases) < 2:
        parser.error("provide baseline and later complete-record prefixes, in order")
    else:
        if len({path.stem for path in args.phases}) != len(args.phases):
            parser.error("phase filename stems must be distinct")
        try:
            result = retained(
                args.phases[-1], {p.stem: p.stat().st_size for p in args.phases}
            )
        except (OSError, ValueError) as error:
            parser.error(str(error))
        print(json.dumps(result))


if __name__ == "__main__":
    main()
