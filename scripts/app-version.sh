#!/usr/bin/env bash
# The one place the app version is read from: the Cargo workspace. CI, the
# release script and the justfile all take it from here — never from a tag,
# package.json or a second file.
set -euo pipefail
cd "$(dirname "$0")/.."
sed -n '/^\[workspace\.package\]/,/^\[/{s/^version = "\(.*\)"$/\1/p}' Cargo.toml | head -1
