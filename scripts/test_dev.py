"""Exercise the dev launcher without touching a running smabar instance."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


@unittest.skipUnless(os.name == "posix", "dev.sh runs on Linux and macOS")
class DevLauncherTests(unittest.TestCase):
    def test_restart_matches_literal_path_and_refuses_busy_port(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve() / "smabar+ (dev)[test]"
            (root / "scripts").mkdir(parents=True)
            (root / "crates/smabar").mkdir(parents=True)
            (root / "shell/node_modules/.bin").mkdir(parents=True)
            shutil.copyfile(Path(__file__).with_name("dev.sh"), root / "scripts/dev.sh")
            commands = root / "commands"
            commands.mkdir()
            pkill = shutil.which("pkill")
            self.assertIsNotNone(pkill)
            stubs = {
                "uname": 'echo "$DEV_TEST_OS"',
                "bun": "exit 0",
                "cargo": "exit 0",
                "uv": "exit 0",
                "sleep": "exit 0",
                "xdotool": "exit 1",
                "ss": '[ "$DEV_TEST_BUSY" = 0 ] || echo "LISTEN 0 128 127.0.0.1:5173 "',
                "lsof": '[ "$DEV_TEST_BUSY" = 1 ]',
                # Only forward this test's Vite pattern to the real pkill.
                "pkill": 'case "$*" in *shell/node_modules*) exec "$DEV_TEST_PKILL" "$@" ;; esac',
            }
            for name, body in stubs.items():
                path = commands / name
                path.write_text(f"#!/bin/bash\n{body}\n")
                path.chmod(0o755)
            tauri = root / "shell/node_modules/.bin/tauri"
            tauri.write_text('#!/bin/bash\nprintf "started: %s\\n" "$@"\n')
            tauri.chmod(0o755)
            env = {
                **os.environ,
                "PATH": f"{commands}:{os.environ['PATH']}",
                "SMABAR_UV": str(commands / "uv"),
                "DEV_TEST_PKILL": pkill,
            }
            for platform in ("Linux", "Darwin"):
                with self.subTest(platform=platform):
                    orphan = subprocess.Popen(
                        [
                            sys.executable,
                            "-c",
                            "import time; time.sleep(60)",
                            str(tauri.with_name("vite")),
                        ]
                    )
                    try:
                        result = subprocess.run(
                            ["bash", "scripts/dev.sh"],
                            cwd=root,
                            env={**env, "DEV_TEST_OS": platform, "DEV_TEST_BUSY": "0"},
                            text=True,
                            capture_output=True,
                            timeout=10,
                            check=False,
                        )
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn("started: dev", result.stdout)
                        orphan.wait(timeout=5)
                        self.assertLess(orphan.returncode, 0)
                        self.assertEqual(
                            "macos-private-api" in result.stdout, platform == "Darwin"
                        )
                    finally:
                        if orphan.poll() is None:
                            orphan.terminate()
                        orphan.wait(timeout=5)
                    result = subprocess.run(
                        ["bash", "scripts/dev.sh"],
                        cwd=root,
                        env={**env, "DEV_TEST_OS": platform, "DEV_TEST_BUSY": "1"},
                        text=True,
                        capture_output=True,
                        timeout=10,
                        check=False,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("port 5173 is still occupied", result.stderr)
                    self.assertNotIn("started: dev", result.stdout)


if __name__ == "__main__":
    unittest.main()
