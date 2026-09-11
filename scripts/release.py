#!/usr/bin/env python3
"""Builds the update release tree (ADR 0009).

    release.py --version V --artifacts DIR [DIR ...] --out DIR --base-url URL
               [--notes-file F] [--require-all] [--dry-run]

Every package found below the artifact directories (.deb, .rpm, *-setup.exe,
*.app.tar.gz) is copied to <out>/<version>/, signed with the minisign key
through `tauri signer sign` unless a .sig already sits beside it, listed in
SHA256SUMS, and announced in <out>/latest.json. A macOS .dmg is treated the
same way but announced as the platform's `download`: humans install the DMG,
the updater installs the .app.tar.gz. The same tree serves updates.smabar.com
and the local dev store; only --base-url differs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "shell" / "node_modules" / ".bin" / "tauri"
# Package suffix -> updater platform key. Linux and Windows ship one
# architecture each; macOS ships two, so its suffixes carry the architecture.
PLATFORMS = {
    ".deb": "linux-deb-x86_64",
    ".rpm": "linux-rpm-x86_64",
    "-setup.exe": "windows-x86_64",
    "_aarch64.app.tar.gz": "darwin-aarch64",
    "_x64.app.tar.gz": "darwin-x86_64",
}
# Human downloads that are not what the updater installs; each needs the
# package of the same key beside it.
DOWNLOADS = {
    "_aarch64.dmg": "darwin-aarch64",
    "_x64.dmg": "darwin-x86_64",
}
SIGNING_VARIABLES = ("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PATH")


def find_by_suffix(directories: list[Path], table: dict[str, str]) -> dict[str, Path]:
    """Key -> file for every file whose name ends in one of the table's suffixes."""
    found: dict[str, Path] = {}
    for directory in directories:
        for file in sorted(directory.rglob("*")):
            if not file.is_file():
                continue
            key = next(
                (key for suffix, key in table.items() if file.name.endswith(suffix)),
                None,
            )
            if key is None:
                continue
            if key in found:
                sys.exit(f"two files for {key}: {found[key]} and {file}")
            found[key] = file
    return found


def find_packages(directories: list[Path]) -> tuple[dict[str, Path], dict[str, Path]]:
    """(platform key -> package, platform key -> download); a DMG without its
    .app.tar.gz is a mistake, because the updater could not install it."""
    packages = find_by_suffix(directories, PLATFORMS)
    if not packages:
        listed = ", ".join(str(directory) for directory in directories)
        sys.exit(f"no package ({', '.join(PLATFORMS)}) found below {listed}")
    downloads = find_by_suffix(directories, DOWNLOADS)
    for key, file in downloads.items():
        if key not in packages:
            sys.exit(f"{file.name} has no {key} .app.tar.gz beside it")
    return packages, downloads


def sign(file: Path) -> str:
    """Content of <file>.sig — created now unless it already exists."""
    signature = file.with_name(file.name + ".sig")
    if not signature.exists():
        subprocess.run(
            [str(TAURI), "signer", "sign", str(file)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    return signature.read_text().strip()


def sha256(file: Path) -> str:
    digest = hashlib.sha256()
    with file.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stage(source: Path, release_dir: Path) -> Path:
    """Copies the file into the release tree; a signature made for other bytes
    must not survive, one made beside the source travels along."""
    target = release_dir / source.name
    if source.resolve() != target.resolve():
        shutil.copy2(source, target)
        target.with_name(target.name + ".sig").unlink(missing_ok=True)
        existing = source.with_name(source.name + ".sig")
        if existing.exists():
            shutil.copy2(existing, target.with_name(target.name + ".sig"))
    return target


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--version", required=True, help="from scripts/app-version.sh")
    parser.add_argument("--artifacts", nargs="+", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--base-url", required=True, help="where <version>/<file> is served"
    )
    parser.add_argument("--notes-file", type=Path)
    parser.add_argument(
        "--require-all",
        action="store_true",
        help="require every package and download of a full release",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the manifest; copy and sign nothing",
    )
    args = parser.parse_args()

    packages, downloads = find_packages(args.artifacts)
    if args.require_all and (
        set(packages) != set(PLATFORMS.values())
        or set(downloads) != set(DOWNLOADS.values())
    ):
        sys.exit(
            "a full release needs one of each: deb, rpm, Windows -setup.exe, and "
            "for macOS aarch64 and x64 both the .app.tar.gz and the .dmg"
        )
    base = args.base_url.rstrip("/")

    def url(file: Path) -> str:
        return f"{base}/{args.version}/{file.name}"

    manifest: dict[str, object] = {
        "version": args.version,
        "notes": args.notes_file.read_text().strip() if args.notes_file else "",
        "pub_date": datetime.now(timezone.utc)
        .isoformat(timespec="seconds")
        .replace("+00:00", "Z"),
    }
    if args.dry_run:
        platforms = {
            key: {"url": url(file), "signature": "<dry-run>"}
            for key, file in packages.items()
        }
        for key, file in downloads.items():
            platforms[key]["download"] = url(file)
        manifest["platforms"] = platforms
        print(json.dumps(manifest, indent=2))
        return
    if not any(os.environ.get(name) for name in SIGNING_VARIABLES):
        sys.exit(
            "set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH "
            "(plus TAURI_SIGNING_PRIVATE_KEY_PASSWORD) to sign packages"
        )
    if not TAURI.exists():
        sys.exit(f"{TAURI} is missing — run `bun install` in shell/")

    release_dir = args.out / args.version
    release_dir.mkdir(parents=True, exist_ok=True)
    platforms = {}
    sums: list[str] = []
    for key, source in packages.items():
        target = stage(source, release_dir)
        platforms[key] = {"url": url(target), "signature": sign(target)}
        sums.append(f"{sha256(target)}  {target.name}\n")
    for key, source in downloads.items():
        target = stage(source, release_dir)
        sign(target)
        platforms[key]["download"] = url(target)
        sums.append(f"{sha256(target)}  {target.name}\n")
    manifest["platforms"] = platforms
    (release_dir / "SHA256SUMS").write_text("".join(sums))
    (args.out / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(
        f"{len(packages) + len(downloads)} file(s) in {release_dir}; "
        f"manifest {args.out / 'latest.json'}"
    )


if __name__ == "__main__":
    main()
