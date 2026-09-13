#!/usr/bin/env python3
"""Sample a Linux process tree as JSONL; all memory values are in KiB."""

import argparse
import json
import math
import sys
import time
from pathlib import Path

FIELDS = ("Pss", "Rss", "Private_Clean", "Private_Dirty", "Anonymous")


def process_tree(root_pid: int, proc: Path) -> dict[int, tuple[int, str, str]]:
    processes = {}
    for directory in proc.iterdir():
        if not directory.name.isdecimal():
            continue
        try:
            stat = (directory / "stat").read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue
        end = stat.rindex(")")
        fields = stat[end + 2 :].split()
        processes[int(directory.name)] = (
            int(fields[1]),
            stat[stat.index("(") + 1 : end],
            fields[19],
        )
    if root_pid not in processes:
        raise ProcessLookupError(f"process {root_pid} no longer exists")
    selected = {root_pid}
    while True:
        children = {
            pid for pid, (parent, _, _) in processes.items() if parent in selected
        }
        expanded = selected | children
        if expanded == selected:
            return {pid: processes[pid] for pid in sorted(selected)}
        selected = expanded


def sample(root_pid: int, proc: Path = Path("/proc")) -> dict:
    tree = process_tree(root_pid, proc)
    rows = []
    missing = []
    totals = dict.fromkeys(FIELDS, 0)
    groups: dict[str, dict[str, int]] = {}
    for pid, (parent, name, started) in tree.items():
        try:
            content = (proc / str(pid) / "smaps_rollup").read_text()
            stat = (proc / str(pid) / "stat").read_text()
        except (FileNotFoundError, ProcessLookupError) as error:
            if pid == root_pid:
                raise ProcessLookupError(
                    "main process exited during measurement"
                ) from error
            missing.append(pid)
            continue
        if stat[stat.rindex(")") + 2 :].split()[19] != started:
            if pid == root_pid:
                raise ProcessLookupError("main PID was reused during measurement")
            missing.append(pid)
            continue
        values = {}
        for line in content.splitlines():
            key, _, rest = line.partition(":")
            if key in FIELDS:
                values[key] = int(rest.split()[0])
        if values.keys() != totals.keys():
            raise ValueError(f"incomplete smaps_rollup for process {pid}")
        if pid == root_pid:
            group = "app"
        elif name.startswith("WebKitWeb"):
            group = "webview"
        elif name.startswith("WebKitNetwork"):
            group = "network"
        elif name == "uv":
            group = "uv"
        elif name.startswith("python"):
            group = "python"
        else:
            group = "other"
        subtotal = groups.setdefault(group, dict.fromkeys(FIELDS, 0))
        for key, value in values.items():
            totals[key] += value
            subtotal[key] += value
        rows.append(
            {"pid": pid, "ppid": parent, "name": name, "group": group, **values}
        )
    return {
        "timestamp": time.time(),
        "root_pid": root_pid,
        "root_start_ticks": tree[root_pid][2],
        "unit": "KiB",
        "processes": rows,
        "groups": groups,
        "total": totals,
        "exited_during_sample": missing,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int, help="smabar main process PID")
    parser.add_argument(
        "--interval", type=float, default=60, help="seconds between samples"
    )
    parser.add_argument("--samples", type=int, default=1)
    args = parser.parse_args()
    if (
        args.pid <= 0
        or not math.isfinite(args.interval)
        or args.interval <= 0
        or args.samples <= 0
    ):
        parser.error(
            "pid, interval and samples must be positive; interval must be finite"
        )
    started = None
    try:
        for index in range(args.samples):
            result = sample(args.pid)
            if started is not None and result["root_start_ticks"] != started:
                raise ProcessLookupError("main PID was reused; start a new measurement")
            started = result["root_start_ticks"]
            print(json.dumps(result), flush=True)
            if index + 1 < args.samples:
                time.sleep(args.interval)
    except (OSError, ValueError) as error:
        print(f"memory measurement failed: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0


if __name__ == "__main__":
    sys.exit(main())
