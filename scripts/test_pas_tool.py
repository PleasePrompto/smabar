"""Offline launch-guard tests; the only possible child target is a sentinel."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent / "memory"


class PasLaunchGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="smabar-pas-guard-")
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.marker = root / "launched"
        target = root / "sentinel.sh"
        target.write_text('#!/bin/sh\n: > "$PAS_GUARD_MARKER"\n')
        target.chmod(0o700)
        hook = root / "hook.so"
        hook.touch()
        collector = root / "collector.so"
        collector.touch()
        self.env = os.environ.copy()
        for name in ("Malloc", "LD_PRELOAD", "SMABAR_HEAPTRACK_DIR"):
            self.env.pop(name, None)
        self.env.update(
            SMABAR_MEMORY_BINARY=str(target),
            SMABAR_PAS_HOOK=str(hook),
            SMABAR_PAS_HEAPTRACK=str(collector),
            SMABAR_PAS_TRACE_DIR=str(root),
            PAS_GUARD_MARKER=str(self.marker),
        )

    def assert_rejected(self, message: str) -> None:
        result = subprocess.run(
            [str(HERE / "run-pas-binary.sh")],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(message, result.stderr)
        self.assertFalse(self.marker.exists(), "Guard executed its child target")

    def test_system_allocator_setting_is_rejected_even_when_empty(self) -> None:
        for value in ("1", ""):
            with self.subTest(value=value):
                self.env["Malloc"] = value
                self.assert_rejected("requires Malloc unset")

    def test_existing_preload_is_rejected(self) -> None:
        self.env["LD_PRELOAD"] = "libm.so.6"
        self.assert_rejected("no other Heaptrack/LD_PRELOAD")

    def test_existing_heaptrack_mode_is_rejected(self) -> None:
        self.env["SMABAR_HEAPTRACK_DIR"] = self.directory.name
        self.assert_rejected("no other Heaptrack/LD_PRELOAD")

    def test_missing_configuration_is_rejected(self) -> None:
        for name in ("SMABAR_PAS_HOOK", "SMABAR_PAS_HEAPTRACK", "SMABAR_PAS_TRACE_DIR"):
            with self.subTest(name=name):
                value = self.env.pop(name)
                self.assert_rejected(name)
                self.env[name] = value

    def test_nonexistent_paths_are_rejected(self) -> None:
        for name in ("SMABAR_PAS_HOOK", "SMABAR_PAS_HEAPTRACK", "SMABAR_PAS_TRACE_DIR"):
            with self.subTest(name=name):
                value = self.env[name]
                self.env[name] = str(Path(self.directory.name) / "missing")
                self.assert_rejected("hook, collector, or trace directory missing")
                self.env[name] = value

    def test_loader_separators_are_rejected(self) -> None:
        for suffix in (" has-space.so", ":other.so"):
            with self.subTest(suffix=suffix):
                self.env["SMABAR_PAS_HOOK"] = str(Path(self.directory.name) / suffix)
                self.assert_rejected("cannot contain whitespace or a colon")


if __name__ == "__main__":
    unittest.main()
