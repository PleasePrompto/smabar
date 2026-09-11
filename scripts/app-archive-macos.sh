#!/usr/bin/env bash
# Packs the built smabar.app as the updater archive the macOS build announces
# in latest.json (ADR 0009): smabar_<version>_<aarch64|x64>.app.tar.gz with
# smabar.app at the archive root, which is what tauri-plugin-updater unpacks.
# Run after `tauri build` on the macOS host that built the app.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(rustc -vV | sed -n 's/^host: //p')" in
  aarch64-apple-darwin) arch=aarch64 ;;
  x86_64-apple-darwin) arch=x64 ;;
  *)
    printf 'error: the updater archive is built on the macOS host that built the app\n' >&2
    exit 2
    ;;
esac
version=$(scripts/app-version.sh)
bundle=target/release/bundle/macos
archive="$bundle/smabar_${version}_${arch}.app.tar.gz"
# No AppleDouble ._* entries: the updater unpacks every entry verbatim into
# the new bundle.
COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$archive" -C "$bundle" smabar.app
printf '%s\n' "$archive"
