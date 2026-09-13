"""Read WebKitPasStatusReporter=2 output without touching the measured process."""

import argparse
import json
import re
from bisect import bisect_left
from itertools import pairwise
from pathlib import Path

HEADER = re.compile(rb"(?m)^(\d+): Heap Status:\n")
SUMMARY = re.compile(
    r"Alloc: (\d+)/(\d+) \(CO\)/(\d+) \(CT\)/(\d+) \(R\); "
    r"Frag: (\d+) \(\d+%\)(?:; Cached: (\d+))?"
)
FIELDS = (
    "allocated",
    "committedObjects",
    "committed",
    "reserved",
    "fragmentation",
    "cached",
)


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def summary_values(line: str) -> dict[str, int] | None:
    match = SUMMARY.search(line)
    if not match:
        return None
    result = dict(zip(FIELDS, (int(value or 0) for value in match.groups())))
    result["freeCommittedObjects"] = result["committedObjects"] - result["allocated"]
    result["committedOutsideObjects"] = result["committed"] - result["committedObjects"]
    result["decommitted"] = result["reserved"] - result["committed"]
    if min(result.values()) < 0:
        raise ValueError(f"Inconsistent PAS summary: {line}")
    return result


def parse_records(data: bytes, pid: int) -> tuple[list[dict], list[dict]]:
    headers = list(HEADER.finditer(data))
    records, rejected = [], []
    for index, header in enumerate(headers):
        if int(header[1]) != pid:
            continue
        end = headers[index + 1].start() if index + 1 < len(headers) else len(data)
        block = data[header.end() : end]
        record = {
            "index": len(records),
            "pid": pid,
            "startByte": header.start(),
            "endByte": end,
            "heaps": {},
            "otherSummaries": {},
        }
        heap = None
        heap_label = None
        for line in block.decode(errors="replace").splitlines():
            if match := re.fullmatch(r"    Heap (0x[0-9a-f]+):", line):
                heap = {"address": match[1], "components": {}}
                heap_label = None
            elif (
                heap is not None and heap_label is None and line.startswith("        ")
            ):
                heap_label = line.strip().split("Type = ")[-1]
                if heap_label in record["heaps"]:
                    raise ValueError(f"Duplicate heap label {heap_label}")
                record["heaps"][heap_label] = heap
            elif heap is not None and line.startswith("        Total Summary:"):
                heap["total"] = summary_values(line)
            elif heap is not None and line.startswith("        "):
                if values := summary_values(line):
                    name = line.strip().split(" 0x")[0]
                    heap["components"][name] = values
            elif line.startswith("    ") and not line.startswith("        "):
                heap = None
                if values := summary_values(line):
                    record["otherSummaries"][line.strip().split(":")[0]] = values
        count = re.search(rb"(?m)^    Num Heaps: (\d+)\n", block)
        complete = (
            count is not None
            and len(record["heaps"]) == int(count[1])
            and all(heap.get("total") for heap in record["heaps"].values())
            and re.search(
                rb"(?m)^    Physical Page Sharing Pool Balance:[^\n]*\n", block
            )
            is not None
        )
        if not complete:
            rejected.append({"startByte": header.start(), "endByte": end})
            continue
        records.append(record)
    return records, rejected


def align_records(
    records: list[dict], offsets: list[dict], samples: list[dict]
) -> None:
    if not offsets or not samples:
        raise ValueError("Offset observations and measurements must both contain rows")
    sizes = [row["bytes"] for row in offsets]
    arrivals = [row["monotonic_seconds"] for row in offsets]
    clocks = [row["monotonic_seconds"] for row in samples]
    if any(
        left > right
        for values in (sizes, arrivals, clocks)
        for left, right in pairwise(values)
    ):
        raise ValueError("Offset sizes and monotonic timestamps must not decrease")
    for record in records:
        # A header observed after the first watch sample has a bounded arrival time.
        first = bisect_left(sizes, record["startByte"] + 1)
        last = bisect_left(sizes, record["endByte"])
        if not first or last >= len(offsets):
            record["timeBounds"] = None
            continue
        lower = offsets[first - 1]["monotonic_seconds"]
        upper = offsets[last]["monotonic_seconds"]
        record["timeBounds"] = {"lower": lower, "upper": upper}
        midpoint = (lower + upper) / 2
        position = bisect_left(clocks, midpoint)
        candidates = [
            index for index in (position - 1, position) if 0 <= index < len(samples)
        ]
        sample_index = min(candidates, key=lambda index: abs(clocks[index] - midpoint))
        record["nearestPhase"] = {
            key: samples[sample_index][key]
            for key in ("cycle", "idle_seconds", "monotonic_seconds")
            if key in samples[sample_index]
        }


def phase_comparisons(records: list[dict], samples: list[dict]) -> dict:
    checkpoints = []
    for sample in samples:
        if (
            sample.get("cycle") not in (0, 100, 200, 300, 400, 500, 600)
            and sample.get("idle_seconds") != 60
        ):
            continue
        clock = sample["monotonic_seconds"]
        preceding = [
            record
            for record in records
            if record.get("timeBounds") and record["timeBounds"]["upper"] <= clock
        ]
        phase = f"cycle-{sample['cycle']}" if "cycle" in sample else "idle-60"
        checkpoint = {"phase": phase, "monotonic_seconds": clock}
        if preceding:
            record = preceding[-1]
            checkpoint.update(
                recordIndex=record["index"],
                maximumAgeSeconds=clock - record["timeBounds"]["lower"],
                nearestPhase=record["nearestPhase"],
            )
        checkpoints.append(checkpoint)
    deltas = []
    for left_name, right_name in (
        ("cycle-100", "cycle-600"),
        ("cycle-400", "cycle-600"),
        ("cycle-600", "idle-60"),
    ):
        left = next((x for x in checkpoints if x["phase"] == left_name), None)
        right = next((x for x in checkpoints if x["phase"] == right_name), None)
        if (
            not left
            or not right
            or "recordIndex" not in left
            or "recordIndex" not in right
        ):
            continue
        old, new = records[left["recordIndex"]], records[right["recordIndex"]]
        heap_deltas = {}
        for name in old["heaps"].keys() & new["heaps"].keys():
            a, b = old["heaps"][name]["total"], new["heaps"][name]["total"]
            heap_deltas[name] = {key: b[key] - a[key] for key in a}
        deltas.append({"from": left_name, "to": right_name, "heaps": heap_deltas})
    return {"checkpoints": checkpoints, "deltas": deltas}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path, help="WebKitPasStatusReporter=2 text output")
    parser.add_argument(
        "--pid",
        type=int,
        required=True,
        help="WebProcess PID printed in the PAS report headers",
    )
    parser.add_argument(
        "--offsets",
        type=Path,
        help="JSONL observations with bytes and monotonic_seconds",
    )
    parser.add_argument(
        "--measurements",
        type=Path,
        help="JSONL OS samples with monotonic_seconds and cycle or idle_seconds; paired with --offsets",
    )
    args = parser.parse_args()
    if bool(args.offsets) != bool(args.measurements):
        parser.error("--offsets and --measurements must be supplied together")
    if args.pid <= 0:
        parser.error("--pid must be positive")
    try:
        data = args.log.read_bytes()
        records, rejected = parse_records(data, args.pid)
        if not records:
            raise ValueError(
                "No complete PAS Level-2 reports found; check --pid and the reporter level"
            )
        result = {"source": str(args.log), "bytesRead": len(data), "records": records}
        result["rejectedIncompleteRecords"] = rejected
        if args.offsets:
            samples = read_jsonl(args.measurements)
            align_records(records, read_jsonl(args.offsets), samples)
            result.update(phase_comparisons(records, samples))
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
