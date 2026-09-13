"""Offline observer CLI and launch-guard checks; never run a WebProcess."""

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent / "memory"
SPEC = importlib.util.spec_from_file_location("pas_tool", HERE / "pas-tool.py")
assert SPEC is not None and SPEC.loader is not None
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


class ObserverModeTests(unittest.TestCase):
    def test_observer_build_does_not_require_heaptrack(self) -> None:
        with tempfile.TemporaryDirectory(prefix="smabar-observer-cli-") as directory:
            cache = Path(directory)
            output = cache / "new-build"
            with (
                patch(
                    "sys.argv",
                    ["pas-tool.py", "--observer", "--test", "--cache-dir", directory],
                ),
                patch.object(TOOL.platform, "system", return_value="Linux"),
                patch.object(TOOL.platform, "machine", return_value="x86_64"),
                patch.object(
                    TOOL,
                    "collector_path",
                    side_effect=AssertionError("Heaptrack was queried"),
                ),
                patch.object(TOOL, "build", return_value=output) as build,
                patch.object(TOOL, "observer_selftest") as test,
            ):
                TOOL.main()
            build.assert_called_once_with(cache, None, observer=True)
            test.assert_called_once_with(output)

    def test_observer_rejects_collector_argument_before_build(self) -> None:
        with (
            patch(
                "sys.argv",
                ["pas-tool.py", "--observer", "--heaptrack-library", "unused"],
            ),
            patch.object(TOOL, "build") as build,
            self.assertRaises(SystemExit) as error,
        ):
            TOOL.main()
        self.assertEqual(error.exception.code, 2)
        build.assert_not_called()


class ObserverLaunchGuardTests(unittest.TestCase):
    def test_invalid_activation_never_executes_child(self) -> None:
        with tempfile.TemporaryDirectory(prefix="smabar-observer-guard-") as directory:
            root = Path(directory)
            marker = root / "launched"
            target = root / "sentinel.sh"
            target.write_text('#!/bin/sh\n: > "$OBSERVER_GUARD_MARKER"\n')
            target.chmod(0o700)
            hook = root / "unused.so"
            hook.touch()
            env = os.environ.copy()
            for name in ("Malloc", "LD_PRELOAD", "SMABAR_HEAPTRACK_DIR"):
                env.pop(name, None)
            env.update(
                SMABAR_NATIVE_OBSERVER_HOOK=str(hook),
                SMABAR_MEMORY_TRACE_DIR=str(root),
                SMABAR_MEMORY_BINARY=str(target),
                OBSERVER_GUARD_MARKER=str(marker),
            )
            for override, message in (
                ({"Malloc": ""}, "requires Malloc unset"),
                ({"LD_PRELOAD": "libm.so.6"}, "no other Heaptrack/LD_PRELOAD"),
                ({"SMABAR_HEAPTRACK_DIR": str(root)}, "no other Heaptrack/LD_PRELOAD"),
                (
                    {"SMABAR_NATIVE_OBSERVER_HOOK": "/tmp/bad path.so"},
                    "cannot contain whitespace",
                ),
                (
                    {"SMABAR_NATIVE_OBSERVER_HOOK": "/tmp/bad:other.so"},
                    "cannot contain whitespace",
                ),
                (
                    {"SMABAR_MEMORY_TRACE_DIR": str(root / "missing")},
                    "output directory missing",
                ),
                ({"SMABAR_MEMORY_TRACE_DIR": ""}, "SMABAR_MEMORY_TRACE_DIR"),
            ):
                with self.subTest(override=override):
                    result = subprocess.run(
                        [str(HERE / "run-binary.sh")],
                        env=env | override,
                        capture_output=True,
                        text=True,
                        timeout=5,
                        check=False,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(message, result.stderr)
                    self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
