"""Process ownership shared by the dev launcher and Linux memory harness."""

import os
import re
import signal
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INSTALLED = {Path("/usr/bin/smabar"), Path("/usr/local/bin/smabar")}


@dataclass(frozen=True)
class Process:
    pid: int
    executable: Path
    cwd: Path
    arguments: tuple[str, ...]


def process_pids() -> list[int]:
    if sys.platform == "darwin":
        result = subprocess.check_output(["ps", "-axo", "pid="], text=True)
        return [int(value) for value in result.split()]
    return [int(path.name) for path in Path("/proc").glob("[0-9]*")]


def read_process(pid: int) -> Process | None:
    try:
        if sys.platform == "darwin":
            result = subprocess.run(
                ["lsof", "-a", "-p", str(pid), "-d", "cwd,txt", "-Ffn"],
                text=True,
                capture_output=True,
                check=False,
            )
            paths = {}
            descriptor = ""
            for line in result.stdout.splitlines():
                if line.startswith("f"):
                    descriptor = line[1:]
                elif line.startswith("n") and descriptor in ("cwd", "txt"):
                    paths.setdefault(descriptor, Path(line[1:]))
            if result.returncode or "cwd" not in paths or "txt" not in paths:
                return None  # Process exited or its executable/cwd cannot be inspected.
            command = subprocess.run(
                ["ps", "-p", str(pid), "-o", "command="],
                text=True,
                capture_output=True,
                check=False,
            )
            if command.returncode:
                return None
            return Process(pid, paths["txt"], paths["cwd"], (command.stdout.strip(),))
        proc = Path("/proc") / str(pid)
        executable = Path(os.readlink(proc / "exe").removesuffix(" (deleted)"))
        cwd = Path(os.readlink(proc / "cwd").removesuffix(" (deleted)"))
        arguments = tuple(
            os.fsdecode(arg)
            for arg in (proc / "cmdline").read_bytes().split(b"\0")
            if arg
        )
        return Process(pid, executable, cwd, arguments)
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None  # Unverifiable/exited processes are never termination candidates.


def is_app(process: Process, root: Path = ROOT) -> bool:
    return process.executable.name == "smabar" and (
        process.executable.is_relative_to(root) or process.executable in INSTALLED
    )


def dev_kind(process: Process, root: Path = ROOT) -> str | None:
    # A known native executable takes precedence over its launch directory.
    if process.executable.name == "smabar":
        return "app" if process.executable.is_relative_to(root) else None
    if not process.cwd.is_relative_to(root):
        return None
    command = " ".join(process.arguments)
    for name in ("tauri", "vite"):
        script = root / "shell/node_modules/.bin" / name
        resolved_script = script.resolve()
        absolute = any(
            re.search(r"(?:^|\s)" + re.escape(str(path)) + r"(?=\s|$)", command)
            for path in (script, resolved_script)
        )
        relative = any(
            arg.endswith(("/" + name, "/" + resolved_script.name))
            and (process.cwd / arg).resolve() == resolved_script
            for arg in (*process.arguments, *command.split())
        )
        if (absolute or relative) and (
            name != "tauri" or re.search(r"(?:^|\s)dev(?:\s|$)", command)
        ):
            return name
    return None


def app_pids(root: Path = ROOT) -> list[int]:
    return [
        pid
        for pid in process_pids()
        if (process := read_process(pid)) and is_app(process, root)
    ]


def stop_dev(root: Path = ROOT) -> None:
    candidates = [
        process
        for pid in process_pids()
        if (process := read_process(pid)) and dev_kind(process, root)
    ]
    # CLI first: it reaps its own Vite/app children before orphan cleanup.
    for process in sorted(candidates, key=lambda item: dev_kind(item, root) != "tauri"):
        if read_process(process.pid) != process:
            continue
        try:
            os.kill(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            continue  # Its parent reaped it between verification and signaling.


def foreign_app(pid: int, root: Path = ROOT) -> bool:
    process = read_process(pid)
    return bool(
        process
        and process.executable.name == "smabar"
        and not is_app(process, root)
        and any(
            (parent / "Cargo.toml").is_file() for parent in process.executable.parents
        )
    )


def blocking_windows(root: Path = ROOT) -> list[str]:
    windows = subprocess.run(
        ["xdotool", "search", "--classname", "^smabar$"],
        text=True,
        capture_output=True,
        check=False,
    )
    blocked = []
    for window in windows.stdout.split():
        owner = subprocess.run(
            ["xdotool", "getwindowpid", window],
            text=True,
            capture_output=True,
            check=False,
        )
        if (
            owner.returncode
            or not owner.stdout.strip().isdigit()
            or not foreign_app(int(owner.stdout), root)
        ):
            blocked.append(window)
    return blocked


if __name__ == "__main__":
    if sys.argv[1:] == ["stop-dev"]:
        stop_dev()
    elif sys.argv[1:] == ["blocking-windows"]:
        print("\n".join(blocking_windows()))
    else:
        raise SystemExit("usage: dev_processes.py stop-dev|blocking-windows")
