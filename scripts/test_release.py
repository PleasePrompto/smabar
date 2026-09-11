"""Release checks run offline; external publication is never part of a test."""

import argparse
import importlib
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import release
from release import DOWNLOADS, PLATFORMS, sha256

r2 = importlib.import_module("release-r2")

# File names as the bundlers produce them, one per platform key.
PACKAGE_NAMES = {
    "linux-deb-x86_64": "smabar_1.0.0_amd64.deb",
    "linux-rpm-x86_64": "smabar-1.0.0-1.x86_64.rpm",
    "windows-x86_64": "smabar_1.0.0_x64-setup.exe",
    "darwin-aarch64": "smabar_1.0.0_aarch64.app.tar.gz",
    "darwin-x86_64": "smabar_1.0.0_x64.app.tar.gz",
}
DOWNLOAD_NAMES = {
    "darwin-aarch64": "smabar_1.0.0_aarch64.dmg",
    "darwin-x86_64": "smabar_1.0.0_x64.dmg",
}


def run_release(source: Path, output: Path, *extra: str) -> None:
    """release.py against a fake signer that writes a distinct .sig per file."""

    def sign(args, **_kwargs):
        file = Path(args[-1])
        file.with_name(file.name + ".sig").write_text(f"sig:{file.name}")

    with (
        patch(
            "sys.argv",
            [
                "release.py",
                "--version",
                "1.0.0",
                "--artifacts",
                str(source),
                "--out",
                str(output),
                "--base-url",
                "https://updates.smabar.com",
                *extra,
            ],
        ),
        patch.dict(os.environ, {"TAURI_SIGNING_PRIVATE_KEY_PATH": "offline-test"}),
        patch.object(release, "TAURI", Path(__file__)),
        patch.object(release.subprocess, "run", side_effect=sign),
        patch("sys.stdout"),
    ):
        release.main()


def full_artifacts(directory: Path) -> Path:
    source = directory / "artifacts"
    source.mkdir()
    for name in [*PACKAGE_NAMES.values(), *DOWNLOAD_NAMES.values()]:
        (source / name).write_bytes(f"bytes of {name}".encode())
    return source


class ReleaseTests(unittest.TestCase):
    def test_suffix_tables_cover_every_bundler_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = full_artifacts(Path(tmp))
            packages = release.find_by_suffix([source], PLATFORMS)
            downloads = release.find_by_suffix([source], DOWNLOADS)
        self.assertEqual({k: f.name for k, f in packages.items()}, PACKAGE_NAMES)
        self.assertEqual({k: f.name for k, f in downloads.items()}, DOWNLOAD_NAMES)

    def test_rebuilding_a_local_package_replaces_its_old_signature(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            source = directory / "input"
            source.mkdir()
            (source / "smabar.deb").write_bytes(b"new package")
            output = directory / "out"
            target = output / "1.0.0"
            target.mkdir(parents=True)
            signature = target / "smabar.deb.sig"
            signature.write_text("old signature")

            def sign(_args, **_kwargs):
                self.assertFalse(signature.exists())
                signature.write_text("new signature")

            with (
                patch(
                    "sys.argv",
                    [
                        "release.py",
                        "--version",
                        "1.0.0",
                        "--artifacts",
                        str(source),
                        "--out",
                        str(output),
                        "--base-url",
                        "http://localhost",
                    ],
                ),
                patch.dict(
                    os.environ, {"TAURI_SIGNING_PRIVATE_KEY_PATH": "offline-test"}
                ),
                patch.object(release, "TAURI", Path(__file__)),
                patch.object(release.subprocess, "run", side_effect=sign),
                patch("sys.stdout"),
            ):
                release.main()
            manifest = json.loads((output / "latest.json").read_text())
            self.assertEqual(
                manifest["platforms"]["linux-deb-x86_64"]["signature"], "new signature"
            )
            self.assertFalse((target / "latest.json").exists())

    def test_a_full_release_announces_the_dmg_as_the_platform_download(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            source = full_artifacts(directory)
            run_release(source, directory / "out", "--require-all")
            manifest = json.loads((directory / "out" / "latest.json").read_text())
            self.assertEqual(set(manifest["platforms"]), set(PACKAGE_NAMES))
            arm = manifest["platforms"]["darwin-aarch64"]
            self.assertEqual(
                arm["url"], "https://updates.smabar.com/1.0.0/smabar_1.0.0_aarch64.app.tar.gz"
            )
            self.assertEqual(arm["signature"], "sig:smabar_1.0.0_aarch64.app.tar.gz")
            self.assertEqual(
                arm["download"], "https://updates.smabar.com/1.0.0/smabar_1.0.0_aarch64.dmg"
            )
            self.assertNotIn("download", manifest["platforms"]["linux-deb-x86_64"])
            release_dir = directory / "out" / "1.0.0"
            listed = {
                line.split("  ", 1)[1]
                for line in (release_dir / "SHA256SUMS").read_text().splitlines()
            }
            self.assertEqual(listed, {*PACKAGE_NAMES.values(), *DOWNLOAD_NAMES.values()})
            self.assertTrue((release_dir / "smabar_1.0.0_x64.dmg.sig").exists())

    def test_a_dmg_without_its_updater_archive_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            source = directory / "artifacts"
            source.mkdir()
            (source / "smabar_1.0.0_x64.dmg").write_bytes(b"dmg")
            (source / "smabar_1.0.0_amd64.deb").write_bytes(b"deb")
            with self.assertRaisesRegex(SystemExit, "no darwin-x86_64 .app.tar.gz"):
                run_release(source, directory / "out")

    def test_require_all_refuses_a_partial_release(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            source = full_artifacts(directory)
            (source / "smabar_1.0.0_x64.dmg").unlink()
            with self.assertRaisesRegex(SystemExit, "a full release needs"):
                run_release(source, directory / "out", "--require-all")


def release_fixture(directory: Path) -> dict:
    """A directory as `gh release download` leaves it: what release.py produced."""
    platforms = {}
    sums = []
    for key, name in PACKAGE_NAMES.items():
        (directory / name).write_bytes(f"bytes of {name}".encode())
        (directory / f"{name}.sig").write_text(f"sig:{name}")
        sums.append(f"{sha256(directory / name)}  {name}\n")
        platforms[key] = {
            "url": f"https://updates.smabar.com/1.0.0/{name}",
            "signature": f"sig:{name}",
        }
    for key, name in DOWNLOAD_NAMES.items():
        (directory / name).write_bytes(f"bytes of {name}".encode())
        (directory / f"{name}.sig").write_text(f"sig:{name}")
        sums.append(f"{sha256(directory / name)}  {name}\n")
        platforms[key]["download"] = f"https://updates.smabar.com/1.0.0/{name}"
    (directory / "SHA256SUMS").write_text("".join(sums))
    manifest = {"version": "1.0.0", "notes": "", "platforms": platforms}
    (directory / "latest.json").write_text(json.dumps(manifest))
    return manifest


class R2Tests(unittest.TestCase):
    def test_version_limits(self):
        self.assertEqual(r2.version_number("1.0.0"), "1.0.0")
        self.assertEqual(r2.version_number("65535.65535.65535"), "65535.65535.65535")
        for value in ("0.1.1", "1.0.0-rc.1", "1.0.0.0", "1.01.0", "65536.0.0", "1.0.0/../x"):
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                r2.version_number(value)

    def test_verify_accepts_a_complete_release_and_names_every_upload(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            release_fixture(directory)
            names = r2.verify(directory, "1.0.0")
            self.assertEqual(len(names), 2 * (len(PACKAGE_NAMES) + len(DOWNLOAD_NAMES)) + 1)
            self.assertIn("smabar_1.0.0_x64.dmg.sig", names)
            self.assertEqual(names[-1], "SHA256SUMS")

    def test_verify_rejects_tampering_and_omissions(self):
        cases = {
            "wrong version": lambda d: None,
            "changed bytes": lambda d: (d / "smabar_1.0.0_amd64.deb").write_bytes(b"x"),
            "missing package": lambda d: (d / "smabar_1.0.0_x64.dmg").unlink(),
            "missing signature": lambda d: (d / "smabar_1.0.0_x64-setup.exe.sig").unlink(),
            "signature mismatch": lambda d: (
                d / "smabar_1.0.0_x64-setup.exe.sig"
            ).write_text("other"),
            "unlisted file": lambda d: (d / "SHA256SUMS").write_text(
                (d / "SHA256SUMS").read_text() + f"{'0' * 64}  extra.deb\n"
            ),
            "foreign url": lambda d: (d / "latest.json").write_text(
                (d / "latest.json")
                .read_text()
                .replace("https://updates.smabar.com/1.0.0/smabar_1.0.0_amd64.deb", "https://x/y.deb")
            ),
        }
        for label, mutate in cases.items():
            with self.subTest(case=label), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                release_fixture(directory)
                mutate(directory)
                version = "1.0.1" if label == "wrong version" else "1.0.0"
                with self.assertRaises(ValueError):
                    r2.verify(directory, version)

    def test_publish_uploads_packages_immutably_then_the_manifest_with_notes(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            release_fixture(directory)
            notes = directory / "body.md"
            notes.write_text("First release.\r\n\r\n- Linux, Windows, macOS Beta\r\n")
            uploaded: dict[str, str | dict] = {}

            def command(args, **_kwargs):
                source, destination = args[3], args[4]
                if destination.endswith("latest.json"):
                    uploaded[destination] = json.loads(Path(source).read_text())
                else:
                    uploaded[destination] = args[-1]

            with (
                patch(
                    "sys.argv",
                    [
                        "release-r2.py",
                        "--version",
                        "1.0.0",
                        "--directory",
                        tmp,
                        "--notes-file",
                        str(notes),
                    ],
                ),
                patch.object(r2.subprocess, "check_output", return_value="{}"),
                patch.object(r2.subprocess, "run", side_effect=command),
                patch("sys.stdout"),
            ):
                r2.main()
            self.assertEqual(
                uploaded["s3://smabar-updates/1.0.0/smabar_1.0.0_amd64.deb"],
                "public, max-age=31536000, immutable",
            )
            manifest = uploaded["s3://smabar-updates/latest.json"]
            self.assertEqual(manifest["notes"], "First release.\n\n- Linux, Windows, macOS Beta")
            self.assertEqual(list(uploaded)[-1], "s3://smabar-updates/latest.json")
            self.assertNotIn("s3://smabar-updates/staging/latest.json", uploaded)

    def test_staging_writes_only_the_staging_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            release_fixture(directory)
            with (
                patch(
                    "sys.argv",
                    [
                        "release-r2.py",
                        "--version",
                        "1.0.0",
                        "--directory",
                        tmp,
                        "--manifest-key",
                        "staging/latest.json",
                    ],
                ),
                patch.object(r2.subprocess, "check_output", return_value="{}"),
                patch.object(r2.subprocess, "run") as command,
                patch("sys.stdout"),
            ):
                r2.main()
            destinations = [call.args[0][4] for call in command.call_args_list]
            self.assertEqual(destinations[-1], "s3://smabar-updates/staging/latest.json")
            self.assertNotIn("s3://smabar-updates/latest.json", destinations)

    def test_a_retry_never_overwrites_a_different_package(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            release_fixture(directory)
            remote = json.dumps(
                {"Contents": [{"Key": "1.0.0/smabar_1.0.0_amd64.deb"}]}
            )

            def command(args, **_kwargs):
                if args[3].startswith("s3://"):
                    Path(args[4]).write_bytes(b"other bytes")

            with (
                patch(
                    "sys.argv",
                    ["release-r2.py", "--version", "1.0.0", "--directory", tmp],
                ),
                patch.object(r2.subprocess, "check_output", return_value=remote),
                patch.object(r2.subprocess, "run", side_effect=command),
                self.assertRaisesRegex(ValueError, "refusing to replace"),
            ):
                r2.main()

    def test_a_typo_cannot_publish_anywhere(self):
        with (
            patch(
                "sys.argv",
                ["release-r2.py", "--version", "1.0.0", "--directory", ".", "--manifest-key", "latest"],
            ),
            patch.object(r2.subprocess, "run") as command,
            patch("sys.stderr"),
            self.assertRaises(SystemExit),
        ):
            r2.main()
        command.assert_not_called()


if __name__ == "__main__":
    unittest.main()
