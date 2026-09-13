#!/usr/bin/env bash
# Last hop before smabar: the dev launcher chain (bun, tauri CLI) drops
# LD_PRELOAD, so heaptrack is attached here when a trace directory is set.
set -euo pipefail
if [[ -n "${SMABAR_HEAPTRACK_DIR:-}" ]]; then
  export LD_PRELOAD=/usr/lib/heaptrack/libheaptrack_preload.so
  export DUMP_HEAPTRACK_OUTPUT="$SMABAR_HEAPTRACK_DIR"'/trace.$$'
fi
exec "${SMABAR_MEMORY_BINARY:?select the measured binary}"
