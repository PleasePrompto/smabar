#!/usr/bin/env python3
"""Build and test opt-in PAS diagnostics; never launch or attach to an app."""

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
GUM_VERSION = "17.9.6"
ARCHIVE = f"frida-gum-devkit-{GUM_VERSION}-linux-x86_64.tar.xz"
URL = f"https://github.com/frida/frida/releases/download/{GUM_VERSION}/{ARCHIVE}"
SHA256 = "2c0d3357ec973a9eecf340350069d52527e304ecab612c13bceea67a657dfe21"
DEFAULT_CACHE = Path.home() / ".cache" / "smabar-memory" / "pas-hooks"


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def checked_output(*command: str) -> str:
    return subprocess.check_output(command, text=True).strip()


def collector_path(override: Path | None) -> Path:
    version = checked_output("heaptrack", "--version")
    if version != "heaptrack 1.5.0":
        raise RuntimeError(f"Expected Heaptrack 1.5.0, found {version!r}")
    if override is not None:
        collector = override.expanduser().resolve()
    else:
        # Reuse the distro package, with no Python environment or second install.
        paths = checked_output("dpkg-query", "-L", "libheaptrack").splitlines()
        matches = [
            Path(path) for path in paths if path.endswith("/libheaptrack_inject.so")
        ]
        if len(matches) != 1:
            raise RuntimeError("Cannot locate collector; pass --heaptrack-library PATH")
        collector = matches[0].resolve()
    if not collector.is_file():
        raise RuntimeError(f"Heaptrack collector does not exist: {collector}")
    return collector


def prepare_devkit(cache: Path) -> Path:
    archive = cache / ARCHIVE
    if not archive.exists():
        partial = archive.with_suffix(".download")
        try:
            print(f"Downloading {URL}", flush=True)
            with (
                urllib.request.urlopen(URL, timeout=60) as source,
                partial.open("wb") as output,
            ):
                shutil.copyfileobj(source, output)
            if digest(partial) != SHA256:
                raise RuntimeError(
                    "Frida devkit SHA256 mismatch; download was not installed"
                )
            partial.replace(archive)
        finally:
            partial.unlink(missing_ok=True)
    if digest(archive) != SHA256:
        raise RuntimeError(f"Frida devkit SHA256 mismatch: remove {archive} and retry")
    devkit = cache / f"frida-gum-{GUM_VERSION}"
    devkit.mkdir(exist_ok=True)
    # Extract only the two regular files needed for this build, never archive paths.
    with tarfile.open(archive) as source:
        for name in ("frida-gum.h", "libfrida-gum.a"):
            member = source.getmember(f"./{name}")
            if not member.isfile():
                raise RuntimeError(f"Unexpected devkit member type: {name}")
            stream = source.extractfile(member)
            if stream is None:
                raise RuntimeError(f"Cannot read devkit member: {name}")
            with stream, (devkit / name).open("wb") as output:
                shutil.copyfileobj(stream, output)
    return devkit


def build(cache: Path, collector: Path | None, observer: bool = False) -> Path:
    devkit = prepare_devkit(cache)
    shim = "webkit-native-observer" if observer else "webkit-pas-hooks"
    test = "native-observer-selftest" if observer else "pas-hook-selftest"
    # Each build gets new paths, so rebuilding never overwrites an in-use shim.
    output = Path(tempfile.mkdtemp(prefix="build-", dir=cache))
    common = [
        "cc",
        "-std=c11",
        "-g",
        "-fno-omit-frame-pointer",
        "-Wall",
        "-Wextra",
        "-Werror",
    ]
    subprocess.run(
        [
            *common,
            "-O2",
            "-fPIC",
            "-shared",
            str(HERE / f"{shim}.c"),
            "-I",
            str(devkit),
            "-o",
            str(output / f"{shim}.so"),
            str(devkit / "libfrida-gum.a"),
            "-lrt",
            "-lresolv",
            "-ldl",
            "-lm",
            "-pthread",
        ],
        check=True,
    )
    subprocess.run(
        [
            *common,
            "-O1",
            "-fno-optimize-sibling-calls",
            str(HERE / f"{test}.c"),
            "-I",
            str(devkit),
            "-o",
            str(output / test),
            *(
                [str(devkit / "libfrida-gum.a"), "-lrt", "-lresolv", "-lm"]
                if observer
                else []
            ),
            "-ldl",
            "-pthread",
        ],
        check=True,
    )
    metadata = {
        "gumVersion": GUM_VERSION,
        "gumUrl": URL,
        "gumSha256": SHA256,
        "mode": "observer" if observer else "allocations",
        "heaptrackLibrary": str(collector) if collector else None,
        "heaptrackSha256": digest(collector) if collector else None,
        "webkitVersion": "2.52.6",
        "abi": "Linux x86_64 System V",
        "shim": str(output / f"{shim}.so"),
        "shimSha256": digest(output / f"{shim}.so"),
        "sources": {
            name: digest(HERE / name)
            for name in (
                (f"{shim}.c", f"{test}.c", "pas-tool.py")
                if observer
                else (
                    "webkit-pas-hooks.c",
                    "pas-hooks.def",
                    "pas-hook-selftest.c",
                    "pas-hook-fail-lock.c",
                    "check-pas-hook-trace.py",
                    "pas-tool.py",
                )
            )
        },
    }
    (output / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return output


def observer_selftest(output: Path) -> None:
    run = Path(tempfile.mkdtemp(prefix="observer-test-", dir=output))
    env = os.environ.copy()
    for name in ("Malloc", "LD_PRELOAD", "SMABAR_MEMORY_TRACE_DIR"):
        env.pop(name, None)
    env["SMABAR_NATIVE_OBSERVER_INTERVAL_MS"] = "100"
    subprocess.run(
        [str(output / "native-observer-selftest"), str(run)],
        env=env,
        check=True,
        timeout=30,
    )
    traces = list(run.glob("native-observer-*.jsonl"))
    if len(traces) != 1 or traces[0].stat().st_mode & 0o777 != 0o600:
        raise RuntimeError("Observer test needs exactly one private JSONL output")
    rows = [json.loads(line) for line in traces[0].read_text().splitlines()]
    ready = rows[0]
    if ready["event"] != "ready" or ready["hooks"] != 11:
        raise RuntimeError("Observer test did not install all 11 hooks")
    snapshots = {r["label"]: r for r in rows if r["event"] == "snapshot"}
    for label in ("synthetic-counts", "selftest-ack", "address-reuse"):
        row = snapshots[label]
        if (row["cache_keys"], row["cache_entries"], row["queue_objects"]) != (
            2,
            4,
            51,
        ):
            raise RuntimeError(f"Observer counts wrong for {label}")
        if row["reader_tid"] != row["main_tid"] or any(
            row[field]
            for field in (
                "lifecycle_errors",
                "thread_errors",
                "layout_errors",
                "discovered_without_ctor",
                "foreign_owner_objects",
            )
        ):
            raise RuntimeError(
                f"Observer lifetime/thread validation failed for {label}"
            )
    if snapshots["intentional-layout-error"]["layout_errors"] != 1:
        raise RuntimeError("Observer failed to report malformed table metadata")
    skipped = snapshots["foreign-owner-skipped"]
    if skipped["foreign_owner_objects"] != 1 or skipped["caches"] != 0:
        raise RuntimeError("Observer failed to exclude foreign-thread cache")
    if list(run.glob("*.request")):
        raise RuntimeError("System GLib timer did not consume the request")
    # The real constructor must ignore unrelated processes despite explicit preload.
    env.update(
        LD_PRELOAD=str(output / "webkit-native-observer.so"),
        SMABAR_MEMORY_TRACE_DIR=str(run),
    )
    subprocess.run(["/bin/true"], env=env, check=True, timeout=5)
    if list(run.glob("*.jsonl")) != traces:
        raise RuntimeError("Observer unexpectedly activated in /bin/true")
    print(
        f"Observer counts, lifetimes, owner-thread exclusion and request ACK passed: {traces[0]}"
    )


def selftest(output: Path, collector: Path) -> None:
    subprocess.run(
        [
            "cc",
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fPIC",
            "-shared",
            str(HERE / "pas-hook-fail-lock.c"),
            "-o",
            str(output / "fail-lock.so"),
        ],
        check=True,
    )
    env = os.environ.copy()
    for name in (
        "Malloc",
        "SMABAR_PAS_TRACE_DIR",
        "SMABAR_PAS_HEAPTRACK",
        "SMABAR_PAS_TEST_CONSTRUCTOR",
    ):
        env.pop(name, None)
    env["LD_PRELOAD"] = str(output / "webkit-pas-hooks.so")
    executable = output / "pas-hook-selftest"
    # Both explicit start and the real before-main constructor must give the same ledger.
    for constructor in (False, True):
        run = Path(
            tempfile.mkdtemp(
                prefix="constructor-" if constructor else "manual-", dir=output
            )
        )
        trace = run / "selftest.raw"
        if constructor:
            env.update(
                SMABAR_PAS_TEST_CONSTRUCTOR="1",
                SMABAR_PAS_TRACE_DIR=str(run),
                SMABAR_PAS_HEAPTRACK=str(collector),
            )
        subprocess.run(
            [str(executable), str(trace), str(collector)],
            env=env,
            check=True,
            timeout=60,
        )

        if constructor:
            traces = list(run.glob("pas-webkit-*.raw"))
            if len(traces) != 1:
                raise RuntimeError("Constructor did not produce exactly one trace")
            trace = traces[0]
        subprocess.run(
            [
                sys.executable,
                str(HERE / "check-pas-hook-trace.py"),
                str(trace),
                "--executable",
                str(executable),
            ],
            check=True,
            timeout=60,
        )
    for name in (
        "SMABAR_PAS_TEST_CONSTRUCTOR",
        "SMABAR_PAS_TRACE_DIR",
        "SMABAR_PAS_HEAPTRACK",
    ):
        env.pop(name, None)
    for scenario in ("existing-trace", "existing-stats", "missing-directory", "locked"):
        run = Path(tempfile.mkdtemp(prefix=f"{scenario}-", dir=output))
        trace = run / "test.raw"
        preserved = None
        if scenario.startswith("existing-"):
            preserved = (
                trace
                if scenario == "existing-trace"
                else Path(str(trace) + ".stats.json")
            )
            preserved.write_text("preserve existing evidence\n")
        elif scenario == "missing-directory":
            trace = run / "missing" / "test.raw"
        else:
            env["LD_PRELOAD"] = (
                f"{output / 'fail-lock.so'}:{output / 'webkit-pas-hooks.so'}"
            )
        result = subprocess.run(
            [str(executable), str(trace), str(collector)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        expected = (
            "initialize collector failed"
            if scenario == "locked"
            else "reserve fresh output failed"
        )
        if (
            result.returncode != 127
            or expected not in result.stderr
            or "PAS diagnostic ready" in result.stderr
        ):
            raise RuntimeError(
                f"Output failure selftest {scenario}: unexpected result {result.returncode}: {result.stderr}"
            )
        if (
            preserved is not None
            and preserved.read_text() != "preserve existing evidence\n"
        ):
            raise RuntimeError(f"Output failure selftest overwrote {preserved}")
    print("Collector lock/open failures and existing trace/stats preservation passed.")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", type=Path, default=DEFAULT_CACHE)
    parser.add_argument(
        "--heaptrack-library",
        type=Path,
        help="Heaptrack 1.5.0 collector, if not found through dpkg",
    )
    parser.add_argument(
        "--test",
        action="store_true",
        help="run headless native checks for the selected diagnostic",
    )
    parser.add_argument(
        "--observer",
        action="store_true",
        help="build the exact-build cache/renderer-queue counter, without Heaptrack",
    )
    args = parser.parse_args()
    if args.observer and args.heaptrack_library is not None:
        parser.error("--observer does not use --heaptrack-library")
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("Only Linux x86_64 System V is supported")
    cache = args.cache_dir.expanduser().resolve()
    if any(character.isspace() or character == ":" for character in str(cache)):
        parser.error(
            "Cache path cannot contain whitespace or ':' (LD_PRELOAD splits these); choose --cache-dir"
        )
    cache.mkdir(parents=True, exist_ok=True, mode=0o700)
    if cache.stat().st_uid != os.getuid() or cache.stat().st_mode & 0o022:
        parser.error(
            "Cache must be owned by the current user and not writable by group/others"
        )
    collector = None if args.observer else collector_path(args.heaptrack_library)
    output = build(cache, collector, observer=args.observer)
    if args.test and args.observer:
        observer_selftest(output)
    elif args.test and collector is not None:
        selftest(output, collector)
    print(f"Build metadata: {output / 'build.json'}")
    if args.observer:
        print(f"SMABAR_NATIVE_OBSERVER_HOOK={output / 'webkit-native-observer.so'}")
    else:
        print(f"SMABAR_PAS_HOOK={output / 'webkit-pas-hooks.so'}")
        print(f"SMABAR_PAS_HEAPTRACK={collector}")


if __name__ == "__main__":
    try:
        main()
    except (
        OSError,
        RuntimeError,
        subprocess.SubprocessError,
        tarfile.TarError,
    ) as error:
        raise SystemExit(f"PAS tool failed: {error}") from error
