#!/usr/bin/env bash
# Runs the measured binary under gdb (its parent, so ptrace_scope 1 allows it).
# SIGUSR1 from the harness stops the process and prints every thread's backtrace.
set -euo pipefail
exec gdb -q -batch -ex "set startup-with-shell off" -ex "set pagination off" \
  -ex "handle SIGUSR1 stop print nopass" -ex "handle SIGPIPE nostop noprint pass" -ex "run" \
  -ex "thread apply all bt" -ex "info threads" -ex "kill" \
  --args "${SMABAR_MEMORY_BINARY:?select the measured binary}" > "${SMABAR_GDB_LOG:?gdb log path}" 2>&1
