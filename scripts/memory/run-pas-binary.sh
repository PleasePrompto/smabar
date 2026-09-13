#!/usr/bin/env bash
# Explicit final launcher hop: keep the allocator shim out of bun/Tauri tooling.
set -euo pipefail
: "${SMABAR_PAS_HOOK:?set to the tested webkit-pas-hooks.so}"
: "${SMABAR_PAS_HEAPTRACK:?set to the checked Heaptrack 1.5.0 collector}"
: "${SMABAR_PAS_TRACE_DIR:?set to an existing private trace directory}"
if [[ -v Malloc || -n "${SMABAR_HEAPTRACK_DIR:-}" || -n "${LD_PRELOAD:-}" ]]; then
  echo 'PAS profiling requires Malloc unset and no other Heaptrack/LD_PRELOAD instrumentation' >&2
  exit 2
fi
if [[ "$SMABAR_PAS_HOOK" =~ [[:space:]:] ]]; then
  echo 'SMABAR_PAS_HOOK cannot contain whitespace or a colon; rebuild with --cache-dir' >&2
  exit 2
fi
if [[ ! -r "$SMABAR_PAS_HOOK" || ! -r "$SMABAR_PAS_HEAPTRACK" || ! -d "$SMABAR_PAS_TRACE_DIR" ]]; then
  echo 'PAS hook, collector, or trace directory missing; run pas-tool.py --test first' >&2
  exit 2
fi
export LD_PRELOAD="$SMABAR_PAS_HOOK"
exec "$(dirname -- "$0")/run-binary.sh" "$@"
