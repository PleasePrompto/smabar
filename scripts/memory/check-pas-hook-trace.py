"""Check paired PAS events and exact native marker stacks in the controlled test."""

import argparse
import json
import subprocess
from collections import Counter
from pathlib import Path


def check(source: Path, executable: Path) -> dict[str, object]:
    if not __debug__:
        raise RuntimeError("Trace assertions require Python without -O/PYTHONOPTIMIZE")
    traces = [(0, 0)]
    live: dict[int, int] = {}
    allocation_traces: list[int] = []
    base = 0
    segments: list[tuple[int, int]] = []
    frees = 0
    for line in source.read_text().splitlines():
        fields = line.split()
        if fields[:3] == ["m", "1", "x"]:
            base = int(fields[3], 16)
            values = [int(value, 16) for value in fields[4:]]
            segments = [
                (base + offset, base + offset + size)
                for offset, size in zip(values[::2], values[1::2], strict=True)
            ]
        elif fields[0] == "t":
            traces.append((int(fields[1], 16), int(fields[2], 16)))
        elif fields[0] == "+":
            size, trace, pointer = (int(value, 16) for value in fields[1:])
            assert pointer not in live, f"Duplicate allocation {pointer:#x}"
            assert size > 0
            live[pointer] = size
            allocation_traces.append(trace)
        elif fields[0] == "-":
            pointer = int(fields[1], 16)
            assert pointer in live, f"Unmatched free {pointer:#x}"
            del live[pointer]
            frees += 1
    assert base and not live
    assert len(allocation_traces) == frees == 793

    offsets = sorted(
        {
            ip - base
            for ip, _ in traces
            if any(start <= ip < end for start, end in segments)
        }
    )
    decoded = subprocess.check_output(
        ["addr2line", "-f", "-e", str(executable), *(hex(ip) for ip in offsets)],
        text=True,
    ).splitlines()[::2]
    symbols = dict(zip(offsets, decoded, strict=True))
    stacks = Counter()
    for trace in allocation_traces:
        first_ip = traces[trace][0]
        names = []
        while trace:
            ip, trace = traces[trace]
            if ip - base in symbols:
                names.append(symbols[ip - base])
        assert symbols.get(first_ip - base) in {
            "allocation_marker_leaf",
            "allocation_marker_zero",
            "main",
        }
        stacks[tuple(names)] += 1
    assert (
        stacks[("allocation_marker_leaf", "allocation_marker_parent", "main", "_start")]
        == 512
    )
    assert (
        stacks[
            (
                "allocation_marker_leaf",
                "allocation_marker_parent",
                "allocation_marker_worker",
            )
        ]
        == 256
    )
    assert stacks[("main", "_start")] == 1
    assert stacks[("allocation_marker_zero", "main", "_start")] == 24
    stats = json.loads(Path(str(source) + ".stats.json").read_text())
    assert stats["allocations"] == stats["frees"] == 793
    assert stats["failures"] == 2
    assert stats["nestedIgnored"] == 196
    assert len(stats["hooks"]) == 31
    assert all(count >= 24 for count in stats["hooks"].values())
    return {
        "allocations": 793,
        "frees": frees,
        "liveBlocks": len(live),
        "unknownFrees": 0,
        "duplicateAllocations": 0,
        "exactCallerStacks": 793,
        "zeroSizeCaseStacks": 24,
        "threadMarkerStacks": 256,
        "hooksExercised": len(stats["hooks"]),
        "failuresWithOwnershipPreserved": 2,
        "nestedCallsIgnored": stats["nestedIgnored"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--executable", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.trace, args.executable), indent=2))
