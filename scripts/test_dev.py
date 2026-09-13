"""Exercise the dev launcher without touching a running smabar instance."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import dev_processes
from dev_processes import Process


@unittest.skipUnless(os.name == "posix", "dev.sh runs on Linux and macOS")
class DevLauncherTests(unittest.TestCase):
    def test_restart_matches_literal_path_and_refuses_busy_port(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve() / "smabar+ (dev)[test]"
            (root / "scripts").mkdir(parents=True)
            (root / "crates/smabar").mkdir(parents=True)
            (root / "shell/node_modules/.bin").mkdir(parents=True)
            shutil.copyfile(Path(__file__).with_name("dev.sh"), root / "scripts/dev.sh")
            shutil.copyfile(
                Path(__file__).with_name("dev_processes.py"),
                root / "scripts/dev_processes.py",
            )
            commands = root / "commands"
            commands.mkdir()
            stubs = {
                "uname": 'echo "$DEV_TEST_OS"',
                "bun": "exit 0",
                "cargo": "exit 0",
                "uv": "exit 0",
                "sleep": "exit 0",
                "xdotool": "exit 1",
                "ss": '[ "$DEV_TEST_BUSY" = 0 ] || echo "LISTEN 0 128 127.0.0.1:5173 "',
                "lsof": 'if [[ "$*" == *"-iTCP:5173"* ]]; then [ "$DEV_TEST_BUSY" = 1 ]; else exec "$DEV_TEST_LSOF" "$@"; fi',
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
                "DEV_TEST_LSOF": shutil.which("lsof") or "lsof",
            }
            for platform in ("Linux", "Darwin"):
                with self.subTest(platform=platform):
                    orphan = subprocess.Popen(
                        [
                            sys.executable,
                            "-c",
                            "import time; time.sleep(60)",
                            str(tauri.with_name("vite")),
                        ],
                        cwd=root,
                    )
                    try:
                        result = subprocess.run(
                            ["bash", "scripts/dev.sh", "--release", "--no-watch"],
                            cwd=root,
                            env={**env, "DEV_TEST_OS": platform, "DEV_TEST_BUSY": "0"},
                            text=True,
                            capture_output=True,
                            timeout=10,
                            check=False,
                        )
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn("started: dev", result.stdout)
                        self.assertIn("started: --release", result.stdout)
                        self.assertIn("started: --no-watch", result.stdout)
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


class ProcessOwnershipTests(unittest.TestCase):
    def test_relative_cli_path_resolves_its_node_modules_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "smabar (dev)"
            script = root / "shell/node_modules/.bin/tauri"
            script.parent.mkdir(parents=True)
            script.symlink_to("../@tauri-apps/cli/tauri.js")
            process = Process(
                1,
                Path("/usr/bin/node"),
                root / "crates/smabar",
                ("../../shell/node_modules/.bin/tauri", "dev"),
            )
            self.assertEqual(dev_processes.dev_kind(process, root), "tauri")

    def test_foreign_executable_wins_over_cwd_and_only_own_tools_are_signaled(self):
        root = Path("/work/smabar+ (dev)[test]")
        foreign = root.with_name(root.name + "-litehtml")
        processes = [
            Process(1, root / "target/debug/smabar", root, ("smabar",)),
            Process(2, foreign / "target/debug/smabar", root, ("smabar",)),
            Process(3, Path("/usr/bin/smabar"), root, ("smabar",)),
            Process(
                4,
                Path("/usr/bin/node"),
                root,
                (str(root / "shell/node_modules/.bin/tauri"), "dev"),
            ),
            Process(
                5,
                Path("/usr/bin/node"),
                foreign,
                (str(foreign / "shell/node_modules/.bin/tauri"), "dev"),
            ),
            Process(
                6,
                Path("/usr/bin/node"),
                root,
                (str(foreign / "shell/node_modules/.bin/vite"),),
            ),
            Process(
                7, Path("/usr/bin/node"), root / "shell", ("node_modules/.bin/vite",)
            ),
        ]
        by_pid = {process.pid: process for process in processes}
        with (
            patch.object(dev_processes, "process_pids", return_value=list(by_pid)),
            patch.object(dev_processes, "read_process", side_effect=by_pid.get),
            patch.object(dev_processes.os, "kill") as kill,
        ):
            self.assertEqual(dev_processes.app_pids(root), [1, 3])
            dev_processes.stop_dev(root)
            self.assertEqual([call.args[0] for call in kill.call_args_list], [4, 1, 7])

    def test_pid_disappearing_or_changing_identity_is_not_signaled(self):
        root = Path("/work/smabar")
        process = Process(1, root / "target/debug/smabar", root, ("smabar",))
        foreign = Process(1, Path("/elsewhere/smabar"), root, ("smabar",))
        for replacement in (None, foreign):
            with (
                self.subTest(replacement=replacement),
                patch.object(dev_processes, "process_pids", return_value=[1]),
                patch.object(
                    dev_processes, "read_process", side_effect=[process, replacement]
                ),
                patch.object(dev_processes.os, "kill") as kill,
            ):
                dev_processes.stop_dev(root)
                kill.assert_not_called()

    def test_window_guard_ignores_only_verified_foreign_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "smabar"
            foreign = Path(directory) / "smabar-litehtml"
            foreign.mkdir()
            (foreign / "Cargo.toml").touch()
            processes = {
                10: Process(10, foreign / "target/debug/smabar", root, ("smabar",)),
                20: Process(20, root / "target/debug/smabar", root, ("smabar",)),
                30: Process(30, Path("/usr/bin/smabar"), root, ("smabar",)),
            }

            def xdotool(args, **_kwargs):
                output = "10\n20\n30\n40\n50\n" if args[1] == "search" else args[-1]
                return subprocess.CompletedProcess(args, 0, output, "")

            with (
                patch.object(dev_processes.subprocess, "run", side_effect=xdotool),
                patch.object(dev_processes, "read_process", side_effect=processes.get),
            ):
                self.assertEqual(
                    dev_processes.blocking_windows(root), ["20", "30", "40", "50"]
                )

    def test_linux_deleted_executable_keeps_ownership_path(self):
        with (
            patch.object(dev_processes.sys, "platform", "linux"),
            patch.object(
                dev_processes.os,
                "readlink",
                side_effect=[
                    "/work/smabar-litehtml/target/debug/smabar (deleted)",
                    "/work/smabar",
                ],
            ),
            patch.object(
                Path, "read_bytes", return_value=b"smabar\0--engine\0litehtml\0"
            ),
        ):
            process = dev_processes.read_process(10)
            self.assertIsNotNone(process)
            self.assertFalse(dev_processes.is_app(process, Path("/work/smabar")))
            self.assertIsNone(dev_processes.dev_kind(process, Path("/work/smabar")))

    def test_darwin_reads_executable_and_cwd_from_lsof(self):
        root = Path("/work/smabar (dev)")
        with (
            patch.object(dev_processes.sys, "platform", "darwin"),
            patch.object(
                dev_processes.subprocess,
                "run",
                side_effect=[
                    subprocess.CompletedProcess(
                        [],
                        0,
                        f"p10\nfcwd\nn{root}\nftxt\nn{root}/target/debug/smabar\nftxt\nn/usr/lib/library.dylib\n",
                        "",
                    ),
                    subprocess.CompletedProcess(
                        [], 0, f"{root}/target/debug/smabar\n", ""
                    ),
                ],
            ),
        ):
            process = dev_processes.read_process(10)
            self.assertIsNotNone(process)
            self.assertTrue(dev_processes.is_app(process, root))


if __name__ == "__main__":
    unittest.main()
