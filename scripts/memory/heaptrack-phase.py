"""Copy a complete-record prefix of a live heaptrack trace for retained-byte diffs."""

import argparse
import json
import tempfile
from pathlib import Path


def snapshot(source: Path, destination: Path) -> dict[str, str | int | None]:
    if source.resolve() == destination.resolve():
        raise ValueError("Source and destination must differ")
    with source.open("rb") as trace:
        size = trace.seek(0, 2)
        trace.seek(max(0, size - 65536))
        tail_start = trace.tell()
        tail = trace.read(size - tail_start)
        newline = tail.rfind(b"\n")
        if newline < 0:
            raise ValueError("No complete record in the final 64 KiB")
        cutoff = tail_start + newline + 1
        elapsed = next(
            (
                int(line.split()[1], 16)
                for line in reversed(tail[: newline + 1].splitlines())
                if line.startswith(b"c ")
            ),
            None,
        )
        trace.seek(0)
        with destination.open("xb") as output:
            remaining = cutoff
            while remaining:
                chunk = trace.read(min(remaining, 1024 * 1024))
                if not chunk:
                    raise OSError("Trace shrank while copying")
                output.write(chunk)
                remaining -= len(chunk)
    return {
        "source": str(source),
        "snapshot": str(destination),
        "bytes": cutoff,
        "lastClockMs": elapsed,
    }


def self_check() -> None:
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / "live.raw"
        destination = Path(directory) / "phase.raw"
        source.write_bytes(b"v 10500 3\nc ff\n+ 20 b 123\n- incomplete")
        result = snapshot(source, destination)
        assert destination.read_bytes() == b"v 10500 3\nc ff\n+ 20 b 123\n"
        assert result["lastClockMs"] == 255
        assert source.read_bytes().endswith(b"- incomplete")
        try:
            snapshot(source, source)
        except ValueError:
            pass
        else:
            raise AssertionError("Source overwrite was accepted")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "source",
        type=Path,
        nargs="?",
        help="uncompressed raw trace, possibly still growing",
    )
    parser.add_argument(
        "destination",
        type=Path,
        nargs="?",
        help="new snapshot path; existing files are never overwritten",
    )
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="run the synthetic copy/overwrite check without a trace",
    )
    args = parser.parse_args()
    if args.self_check:
        self_check()
        print("heaptrack phase snapshot self-check passed")
    elif args.source is None or args.destination is None:
        parser.error("source and destination are required")
    else:
        try:
            result = snapshot(args.source, args.destination)
        except (OSError, ValueError) as error:
            parser.error(str(error))
        print(json.dumps(result))


if __name__ == "__main__":
    main()
