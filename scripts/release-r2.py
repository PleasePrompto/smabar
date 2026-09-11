#!/usr/bin/env python3
"""Publishes verified release bytes to R2; existing version objects are immutable.

    release-r2.py --version V --directory DIR [--notes-file F]
                  [--manifest-key latest.json]

The directory holds what the GitHub release carries: packages, their .sig
files, SHA256SUMS and latest.json. Everything the manifest announces is
checked against the files and SHA256SUMS before a byte leaves the machine.
Packages land under <version>/ and are never replaced. The manifest, with the
release notes filled in, is written to --manifest-key: `latest.json` for
every installation, `staging/latest.json` to test the update path first.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
from pathlib import Path

from release import sha256

BUCKET = "smabar-updates"
BASE_URL = "https://updates.smabar.com"
STABLE_VERSION = re.compile(r"^[1-9][0-9]*\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")


def version_number(value: str) -> str:
    """Stable X.Y.Z with major >= 1 and every part <= 65535 (the MSIX limit)."""
    if not STABLE_VERSION.match(value) or any(
        int(part) > 65535 for part in value.split(".")
    ):
        raise argparse.ArgumentTypeError(f"not a stable X.Y.Z release version: {value}")
    return value


def verify(directory: Path, version: str) -> list[str]:
    """Names of the files to upload, or ValueError naming the first mismatch."""
    manifest = json.loads((directory / "latest.json").read_text())
    if manifest.get("version") != version:
        raise ValueError(f"latest.json announces {manifest.get('version')}, not {version}")
    sums: dict[str, str] = {}
    for line in (directory / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        sums[name] = digest
    prefix = f"{BASE_URL}/{version}/"
    names: set[str] = set()
    for key, entry in manifest["platforms"].items():
        for field in ("url", "download"):
            url = entry.get(field)
            if url is None:
                if field == "url":
                    raise ValueError(f"{key} has no url")
                continue
            name = url.removeprefix(prefix)
            if name == url or "/" in name:
                raise ValueError(f"{key} {field} must be {prefix}<file>: {url}")
            file = directory / name
            if not file.is_file():
                raise ValueError(f"{name} is announced by latest.json but missing")
            if sums.get(name) != sha256(file):
                raise ValueError(f"SHA256SUMS does not match {name}")
            signature = directory / f"{name}.sig"
            if not signature.is_file():
                raise ValueError(f"{name}.sig is missing")
            if field == "url" and signature.read_text().strip() != entry["signature"]:
                raise ValueError(f"{name}.sig differs from the signature in latest.json")
            names.update({name, f"{name}.sig"})
    unlisted = {name for name in names if not name.endswith(".sig")} ^ set(sums)
    if unlisted:
        raise ValueError(f"SHA256SUMS and latest.json disagree about {sorted(unlisted)}")
    return sorted(names) + ["SHA256SUMS"]


def upload_immutable(directory: Path, version: str, names: list[str]) -> None:
    listed = json.loads(
        subprocess.check_output(
            ["aws", "s3api", "list-objects-v2", "--bucket", BUCKET, "--prefix", f"{version}/"],
            text=True,
        )
    )
    remote = {item["Key"] for item in listed.get("Contents", [])}
    with tempfile.TemporaryDirectory() as tmp:
        for name in names:
            file = directory / name
            key = f"{version}/{name}"
            url = f"s3://{BUCKET}/{key}"
            if key in remote:
                cached = Path(tmp) / name
                subprocess.run(
                    ["aws", "s3", "cp", url, str(cached), "--only-show-errors"], check=True
                )
                if sha256(cached) != sha256(file):
                    raise ValueError(f"refusing to replace R2 object {key}; use a new version")
                continue
            subprocess.run(
                [
                    "aws",
                    "s3",
                    "cp",
                    str(file),
                    url,
                    "--only-show-errors",
                    "--cache-control",
                    "public, max-age=31536000, immutable",
                ],
                check=True,
            )


def upload_manifest(directory: Path, key: str, notes: str | None) -> None:
    manifest = json.loads((directory / "latest.json").read_text())
    if notes is not None:
        manifest["notes"] = notes
    with tempfile.TemporaryDirectory() as tmp:
        file = Path(tmp) / "latest.json"
        file.write_text(json.dumps(manifest, indent=2) + "\n")
        subprocess.run(
            [
                "aws",
                "s3",
                "cp",
                str(file),
                f"s3://{BUCKET}/{key}",
                "--only-show-errors",
                "--content-type",
                "application/json",
                "--cache-control",
                "max-age=60",
            ],
            check=True,
        )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--version", required=True, type=version_number)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--notes-file", type=Path, help="release notes for the manifest")
    parser.add_argument(
        "--manifest-key",
        default="latest.json",
        choices=["latest.json", "staging/latest.json"],
        help="where the manifest goes: production, or staging to test first",
    )
    args = parser.parse_args()
    names = verify(args.directory, args.version)
    # GitHub stores release bodies with CRLF; the shell renders plain text.
    notes = (
        args.notes_file.read_text().replace("\r\n", "\n").strip()
        if args.notes_file
        else None
    )
    upload_immutable(args.directory, args.version, names)
    upload_manifest(args.directory, args.manifest_key, notes)
    print(f"{len(names)} file(s) under {args.version}/; manifest at {args.manifest_key}")


if __name__ == "__main__":
    main()
