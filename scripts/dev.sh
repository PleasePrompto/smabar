#!/usr/bin/env bash
# The ONLY way to start smabar in dev: kills every stray instance first,
# verifies the desktop is clean, then starts exactly one.
# (Patterns live in this file, not in a shell cmdline, so pkill/pgrep
# cannot self-match the caller.)
set -euo pipefail
cd "$(dirname "$0")/.."

platform=$(uname -s)
case "$platform" in
  Linux) port_tool=ss ;;
  Darwin) port_tool=lsof ;;
  *) echo "error: use scripts\\dev.bat on Windows; unsupported dev host: $platform" >&2; exit 1 ;;
esac
for tool in "$port_tool" bun cargo "${SMABAR_UV:-uv}"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: $tool is missing; install the development tools listed in README.md" >&2
    exit 1
  fi
done

port_busy() {
  if [[ "$platform" == Darwin ]]; then
    lsof -nP -iTCP:5173 -sTCP:LISTEN -t >/dev/null
  else
    ss -tln | grep -q ':5173 '
  fi
}

# 1. Stop the tauri dev CLI first (it reaps its vite + app children) …
pkill -f 'node_modules/.bin/tauri dev' 2>/dev/null || true
# … then any orphaned app binaries …
pkill -f 'target/debug/smabar' 2>/dev/null || true
# … and orphaned vite dev servers of THIS repo (killing the CLI orphans its
# beforeDevCommand child; a stale vite keeps port 5173 with a stale module
# graph and the bar loads broken code from it).
# pkill takes an extended regex; project paths (including +, brackets and
# parentheses) must be literal. Escape once, including the .bin directory.
vite_pattern=$(printf '%s' "$PWD/shell/node_modules/.bin/vite" | sed 's/[][\\.^$*+?(){}|]/\\&/g')
pkill -f -- "$vite_pattern" 2>/dev/null || true

# Wait until port 5173 is actually free again.
for _ in $(seq 1 20); do
  if ! port_busy; then break; fi
  sleep 0.5
done
if port_busy; then
  echo "error: port 5173 is still occupied; stop its listener before restarting smabar" >&2
  exit 1
fi

# 2. On X11, wait until no smabar window is left on screen (max 10 s). Match
# the WM_CLASS instance: a title match also hits a file manager showing a
# folder named smabar.
if [[ "$platform" == Linux ]] && command -v xdotool >/dev/null 2>&1; then
  for _ in $(seq 1 20); do
    if ! xdotool search --classname '^smabar$' >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done
  if xdotool search --classname '^smabar$' >/dev/null 2>&1; then
    echo "error: stale smabar window(s) still on screen — refusing to start a second instance" >&2
    xdotool search --classname '^smabar$' | while read -r w; do
      xdotool getwindowgeometry "$w" | tr '\n' ' ' >&2
      echo >&2
    done
    exit 1
  fi
fi

echo "clean — starting single dev instance"
# Dev Python tooling (bundled builds resolve both beside/from app resources).
export SMABAR_SDK_PATH="$PWD/sdk/python"
export SMABAR_UV="${SMABAR_UV:-$(command -v uv)}"
# Update checks go to the local store (`just dev-store`), never to the
# release endpoint; debug builds accept plain http there.
export SMABAR_UPDATE_ENDPOINT="${SMABAR_UPDATE_ENDPOINT:-http://127.0.0.1:8787/updates/latest.json}"
cd crates/smabar
if [[ "$platform" == Darwin ]]; then
  # Transparency is configured in Cargo/Tauri too, so direct native checks
  # and source syncs agree. Only the missing Darwin sidecar is skipped here.
  exec ../../shell/node_modules/.bin/tauri dev \
    --features tauri/macos-private-api \
    --config '{"app":{"macOSPrivateApi":true},"bundle":{"externalBin":[]}}' "$@"
fi
exec ../../shell/node_modules/.bin/tauri dev "$@"
