#!/usr/bin/env bash
# Local server for testing application updates without a public repository
# (ADR 0009). Serves .dev-store/ on
# 127.0.0.1:8787; dev.sh points the update check here. Edit latest.json to
# fake a release; a fresh checkout gets one that announces 1.0.1.
set -euo pipefail
cd "$(dirname "$0")/.."

mkdir -p .dev-store/updates
if [[ ! -f .dev-store/updates/latest.json ]]; then
  cat >.dev-store/updates/latest.json <<'JSON'
{
  "version": "1.0.1",
  "notes": "Fake release served from .dev-store/updates/latest.json.",
  "pub_date": "2026-08-29T08:00:00Z",
  "platforms": {
    "linux-deb-x86_64": {
      "url": "http://127.0.0.1:8787/updates/smabar_1.0.1_amd64.deb",
      "signature": "dev-placeholder"
    },
    "linux-rpm-x86_64": {
      "url": "http://127.0.0.1:8787/updates/smabar-1.0.1-1.x86_64.rpm",
      "signature": "dev-placeholder"
    },
    "windows-x86_64": {
      "url": "http://127.0.0.1:8787/updates/smabar_1.0.1_x64-setup.exe",
      "signature": "dev-placeholder"
    },
    "darwin-aarch64": {
      "url": "http://127.0.0.1:8787/updates/smabar_1.0.1_aarch64.app.tar.gz",
      "signature": "dev-placeholder",
      "download": "http://127.0.0.1:8787/updates/smabar_1.0.1_aarch64.dmg"
    },
    "darwin-x86_64": {
      "url": "http://127.0.0.1:8787/updates/smabar_1.0.1_x64.app.tar.gz",
      "signature": "dev-placeholder",
      "download": "http://127.0.0.1:8787/updates/smabar_1.0.1_x64.dmg"
    }
  }
}
JSON
fi
exec python3 -m http.server 8787 --bind 127.0.0.1 --directory .dev-store
