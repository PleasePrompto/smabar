#!/usr/bin/env bash
# Last hop before smabar: the dev launcher chain (bun, tauri CLI) drops
# LD_PRELOAD, so heaptrack is attached here when a trace directory is set.
set -euo pipefail
if [[ -n "${SMABAR_NATIVE_OBSERVER_HOOK:-}" ]]; then
  : "${SMABAR_MEMORY_TRACE_DIR:?set to an existing private observer directory}"
  if [[ -v Malloc || -n "${SMABAR_HEAPTRACK_DIR:-}" || -n "${LD_PRELOAD:-}" ]]; then
    echo 'Native observer requires Malloc unset and no other Heaptrack/LD_PRELOAD instrumentation' >&2
    exit 2
  fi
  if [[ "$SMABAR_NATIVE_OBSERVER_HOOK" =~ [[:space:]:] ]]; then
    echo 'SMABAR_NATIVE_OBSERVER_HOOK cannot contain whitespace or a colon' >&2
    exit 2
  fi
  if [[ ! -r "$SMABAR_NATIVE_OBSERVER_HOOK" || ! -d "$SMABAR_MEMORY_TRACE_DIR" ]]; then
    echo 'Native observer hook or output directory missing; run pas-tool.py --observer --test first' >&2
    exit 2
  fi
  export LD_PRELOAD="$SMABAR_NATIVE_OBSERVER_HOOK"
fi
if [[ -n "${SMABAR_HEAPTRACK_DIR:-}" ]]; then
  export LD_PRELOAD=/usr/lib/heaptrack/libheaptrack_preload.so
  export DUMP_HEAPTRACK_OUTPUT="$SMABAR_HEAPTRACK_DIR"'/trace.$$'
fi
exec "${SMABAR_MEMORY_BINARY:?select the measured binary}"
