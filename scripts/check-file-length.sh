#!/usr/bin/env bash
# Enforces the 500-line limit from CLAUDE.md for smabar source files.
set -euo pipefail
cd "$(dirname "$0")/.."

limit=500
fail=0
while IFS= read -r file; do
  count=$(wc -l <"$file")
  if ((count > limit)); then
    echo "LINE LIMIT: $file has $count lines (max $limit)"
    fail=1
  fi
done < <(find crates/*/src shell/src sdk/python \
  -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.tsx' -o -name '*.py' \) \
  -not -path '*/node_modules/*' -not -path '*/.venv/*')

((fail == 0)) && echo "line limit OK (all files <= $limit lines)"
exit "$fail"
