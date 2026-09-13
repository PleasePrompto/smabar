# Memory harness (Linux)

Reproducible RAM windows against one smabar binary, launched through
`scripts/dev.sh` with the six-plugin user profile. Output lands in
`$SMABAR_MEMORY_OUT` (default `.debug/memory/`, git-ignored). Needs
`xdotool`, `gdbus`, Python 3, and for the inspector scripts `bun` plus a
devtools build started with `WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9228`.

The single-instance guard counts binaries inside this checkout and the standard
installed `/usr/bin/smabar` or `/usr/local/bin/smabar`. A same-named binary from
another checkout is excluded by its executable path, including deleted binaries
still running after a rebuild. The dev launcher shares this ownership check;
it never terminates another checkout's processes. Unknown or installed smabar
windows still block startup rather than being dismissed.

| Script                 | Purpose                                                                                                  |
| ---------------------- | -------------------------------------------------------------------------------------------------------- |
| `cycle-run.py`         | One window: PSS/Anonymous every 5 s per group; `--cycle` opens a flyout every 10 s, `--smaps` mappings, `--native` captures, `--heaptrack` allocation trace of the app process |
| `inspect-run.py`       | Same cycle under a devtools build; `inspect-cycle.js` records WebKit memory categories and heap snapshots, `inspect-scripts.js` the evaluated JS programs and network requests |
| `quit-loop.py`         | Tray quits under gdb (`run-gdb.sh`); a hang leaves a thread dump                                          |
| `session.py`, `mcp_client.py`, `mcp-call.py`, `run-binary.sh` | Shared start/quit, MCP client, launcher hop that keeps `LD_PRELOAD` for heaptrack |
| `pas-tool.py`, `webkit-pas-hooks.c`, `pas-hooks.def` | Build an explicitly activated WebProcess allocation profiler that preserves PAS/bmalloc |
| `pas-hook-selftest.c`, `check-pas-hook-trace.py` | Headless native allocation/lifetime and exact caller-stack checks |
| `webkit-native-observer.c`, `native-observer-selftest.c` | Exact-build native cache/renderer-queue counts, lifecycle checks and system-GLib request acknowledgements |
| `run-pas-binary.sh` | Explicit PAS profiling launcher hop; normal launches do not load the shim |
| `heaptrack-phase.py`, `heaptrack-retained.py`, `pas-status-parse.py` | Offline trace prefixes, retained allocation deltas/audits, and PAS Level 2 reports |

Examples:

```bash
python3 scripts/memory/cycle-run.py target/release/smabar control --probe observe --cycle --minutes 2 --smaps
python3 scripts/memory/cycle-run.py target/release/smabar same-tile --cycle --tile plugin:clock:clock --minutes 2
python3 scripts/memory/inspect-run.py target/release/smabar categories 12 inspect-cycle.js
python3 scripts/memory/quit-loop.py target/release/smabar 20 120
```

Compare the `webview` group of two `<label>.jsonl` files (PSS every 30 s is
every sixth sample). Growth per flyout cycle is the fingerprint of the Linux
ramp; see the memory notes in `docs/` for the recorded results.


With `--probe observe`, the central `~/.smabar/logs/smabar.log.<date>` records
`memory probe flyout` (native generation/phase), `memory probe host mounted`
and `memory probe host cleanup` (same generation, per-render instance, DOM
counts, managed clock groups), and interval samples per plugin/source/surface.
Native lifecycle events include `app_pid`; correlate shell events by timestamp
and flyout generation. A cleanup record's DOM counts describe its original
mount snapshot; the interval sample counts the currently connected DOM.
These logs contain identifiers and numeric sizes, never HTML or field values.

To collect the process tree of an already running diagnostic release without
restarting it:

```bash
python3 scripts/measure-memory.py <current-smabar-pid> --interval 1 --samples 601 > .debug/memory/processes.jsonl
```

The output includes wall-clock `timestamp`, `monotonic_seconds`, and each
process's `start_ticks` so reused child PIDs remain distinguishable. Compare
`Anonymous`/`Pss` in KiB, rather than summing shared-library RSS as private RAM.
`SMABAR_MEMORY_PROBE` activates shell/native counters only; the separate sampler
measures the OS processes. Browser heap ownership still requires a native
allocation profile—connected DOM counts cannot see WebKit's native caches.

## PAS allocation stacks without changing the allocator

This diagnostic currently supports **Linux x86_64 System V ABI, WebKitGTK/JSC
4.1 version 2.52.6, Heaptrack 1.5.0, and Frida Gum 17.9.6**. It depends on
private C++ allocation ABIs, including the hidden return buffer of
`TryMallocReturnValue`. The shim checks JSC's runtime version and refuses a
disabled PAS allocator. Revalidate the symbols, ABI and stack assertions before
changing these versions. This is a development tool with high CPU/memory
overhead and potentially large raw output; instrumented OS RAM is not a normal
application RAM benchmark.

Build and execute the controlled native tests:

```bash
python3 scripts/memory/pas-tool.py --test
python3 -m unittest discover -s scripts -p 'test_pas_tool.py'
```

Needs Python 3.11+, a C compiler, binutils (`addr2line`), the installed
`libjavascriptcoregtk-4.1.so.0`, and Heaptrack 1.5.0. The existing system
Heaptrack package is reused: on Debian/Ubuntu `dpkg-query -L libheaptrack`
locates `libheaptrack_inject.so`. Elsewhere supply
`--heaptrack-library /absolute/path/libheaptrack_inject.so` from the matching
Heaptrack install. No pip environment, global installation, app launch,
process attach, or security-setting change is performed by this command.

The build downloads the [official Gum 17.9.6 C devkit](https://github.com/frida/frida/releases/tag/17.9.6)
and checks SHA256
`2c0d3357ec973a9eecf340350069d52527e304ecab612c13bceea67a657dfe21`
before extracting its header and static library. Dependencies, binaries and
selftest traces stay outside the checkout under
`~/.cache/smabar-memory/pas-hooks`; `--cache-dir` changes that location.
The cache path must have no whitespace or colon because `LD_PRELOAD` splits
those characters. Each build creates a fresh directory and prints its
`build.json`, shim and collector paths. Metadata records source, dependency
and shim hashes; rebuilding never overwrites an existing measurement binary.
Neither the devkit nor trace data belongs in source control.

Both manual activation and the before-main constructor are tested against the
real PAS allocator. Each test asserts 793 allocation/free pairs, no duplicate
or unmatched pointers, 793 correct caller stacks, all 31 hooks, two worker
threads, zero-size requests, overflow failure and failed realloc preserving
the original allocation. Negative tests reject pre-existing trace/stats files,
missing output directories and an injected failure of the real collector’s
`flock` call. The small `pas-hook-fail-lock.c` library is used only by this
headless negative test; it is never part of the profiling runner. Builds use `-Wall -Wextra -Werror`. A trace can be
checked again with:

```bash
python3 scripts/memory/check-pas-hook-trace.py /path/to/selftest.raw \
  --executable /path/to/build/pas-hook-selftest
```

The hook set covers 26 exported WTF functions: normal/Compact malloc,
zeroed malloc, calloc, realloc, aligned allocation, their try variants,
StrDup/MemDup, `fastFree` and `fastAlignedFree`; plus five
`bmalloc::api::iso*` entrypoints. Nested wrappers count once, and diagnostic
recursion is excluded per thread. Successful allocations are recorded after
return; frees before deallocation. Realloc changes the ledger only on
success. Sizes are PAS allocation sizes, including size-class rounding.
[WTF implementation](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WTF/wtf/FastMalloc.cpp),
[Iso entrypoints](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/bmalloc/bmalloc/IsoHeap.h).

The existing [Heaptrack custom allocator API](https://github.com/KDE/heaptrack/blob/v1.5.0/src/track/heaptrack_api.h)
records allocation stacks and allocation/free lifetime events. Its libc
interposition/injection entrypoint is not called. Gum captures accurate
native stacks with a **16-frame maximum**; a conditional bridge supplies
these frames to Heaptrack's public `unw_backtrace` call, accounting for
[Heaptrack 1.5.0's two skipped frames](https://github.com/KDE/heaptrack/blob/v1.5.0/src/track/trace.h).
No fuzzy frames or free stacks are recorded. Direct inlined bmalloc/PAS calls
that bypass the exported WTF/Iso functions remain outside this coverage, as do
other allocators, mappings and driver memory. Individual JSC GC cells are not
enumerated; some backing blocks appear through hooked WTF paths. Loader allocations
before the constructor are also absent. Compare traced live-byte deltas with
PAS Common Primitive allocated-byte deltas; exported hooks cannot prove
complete allocator coverage.

## Explicit PAS activation

The normal launcher is unchanged. For a deliberate measurement, close the
existing instance through its normal UI first and use the dedicated final
runner. Set the shim and collector paths printed by the successful build/test:

```bash
mkdir -p -m 700 .debug/memory/pas
env -u Malloc \
  SMABAR_MEMORY_BINARY="$(realpath target/release/smabar)" \
  SMABAR_PAS_HOOK=/absolute/path/to/build/webkit-pas-hooks.so \
  SMABAR_PAS_HEAPTRACK=/usr/lib/heaptrack/libheaptrack_inject.so \
  SMABAR_PAS_TRACE_DIR="$PWD/.debug/memory/pas" \
  scripts/dev.sh --no-watch --no-dev-server \
  --config '{"build":{"devUrl":null,"beforeDevCommand":""}}' \
  --runner "$PWD/scripts/memory/run-pas-binary.sh"
```

The runner sets `LD_PRELOAD` only at the last hop. It rejects `Malloc`, another
`LD_PRELOAD`, and `SMABAR_HEAPTRACK_DIR`; do not combine this with
`cycle-run.py --heaptrack`. Only the child executable named `WebKitWebProcess`
activates hooks. Each writes `pas-webkit-<PID>.raw` and a readiness diagnostic
before main; normal exit writes `<trace>.stats.json`. The output directory
must already exist and be accessible to the child. Capture from process start:
starting midway would omit the allocations behind later cache evictions.

## Offline phases and symbolized stacks

All commands below operate on files and leave running processes alone.
Snapshot a live raw trace at two chosen boundaries:

```bash
python3 scripts/memory/heaptrack-phase.py .debug/memory/trace.raw .debug/memory/warm.raw
python3 scripts/memory/heaptrack-phase.py .debug/memory/trace.raw .debug/memory/idle.raw
python3 scripts/memory/heaptrack-retained.py .debug/memory/warm.raw .debug/memory/idle.raw \
  > .debug/memory/retained.json
```

Prefix creation copies complete lines to a new file and refuses overwrite;
stdout reports bytes and the last available clock. Retained analysis requires
uncompressed prefixes of the **same trace**, in chronological order, with
distinct file stems. It reads the last file; earlier sizes define boundaries.
Inspect `eventAudits` and `eventAuditIntervals`: unmatched frees, duplicate
live pointers and overwritten tracked bytes limit the ledger's interpretation.
Retained bytes mean observed allocations not yet freed, not GC reachability
or a proven leak. Both helpers offer `--self-check`.

The Python ledger reports addresses. Use the installed Heaptrack interpreter
and printer for symbols and complete recorded chains; these commands were
verified on the native selftest trace:

```bash
/usr/lib/heaptrack/libexec/heaptrack_interpret < .debug/memory/idle.raw \
  > .debug/memory/idle.heaptrack
heaptrack_print .debug/memory/idle.heaptrack --print-peaks 0 --print-temporary 0 \
  --print-allocators 1 > .debug/memory/allocators.txt
heaptrack_print .debug/memory/idle.heaptrack --print-peaks 0 --print-temporary 0 \
  --print-allocators 0 --print-leaks 1 > .debug/memory/retained-stacks.txt
```

`heaptrack_print` calls remaining allocations "leaks"; this includes live
caches and deferred deletion. Keep the exact binaries and matching build-ID
debug information available at interpretation time. Exported symbols can
resolve from the libraries themselves; private functions and source lines
need the matching distribution dbgsym/debuginfo files. Do not substitute a
different WebKit build. Interpreter location is distribution-specific; the
path above is provided by the installed Debian/Ubuntu Heaptrack package.

For captured PAS Level 2 diagnostic text:

```bash
python3 scripts/memory/pas-status-parse.py .debug/memory/pas.log --pid <WebProcess-PID> \
  --offsets .debug/memory/pas-offsets.jsonl \
  --measurements .debug/memory/cycle-phases.jsonl > .debug/memory/pas.json
```

Supply both time files or neither. Offsets need `bytes` and
`monotonic_seconds`; measurements need `monotonic_seconds` plus `cycle` or
`idle_seconds`. The measurement harness must enrich these phase lines explicitly;
the existing `cycle-run.py` and ordinary OS sampler do not write these cycle
fields. Unknown early time boundaries remain null; incomplete reports are
reported. All reports are retained, with comparisons for available
100→600, 400→600 and 600→60-second-idle phases. Do not add heap components
to their aggregate rows; PAS `allocated` is accounting, not an ownership proof.

## Native cache and renderer-queue observer

`pas-tool.py --observer` reuses the pinned Gum download and fresh build directories
above. It needs **the exact Linux x86_64 WebKitGTK 4.1 ELF build ID
`6d0db877d1bda65539bf0c84fd6f72773b56ed4b`** (version 2.52.6), its installed JSC
and system GLib libraries. Heaptrack is not required or loaded. No app, WebView,
window or debugger attach is started by the build/test command:

```bash
python3 scripts/memory/pas-tool.py --observer --test
python3 -m unittest discover -s scripts -p 'test_*tool.py'
```

The constructor validates the actual WebKit build ID, all eleven private hook
prologues and the normal PAS allocator. It fails explicitly on an unsupported
build; the same version string alone is insufficient. The output records the
build ID and module base. Revalidate offsets against matching source and debug
symbols before supporting another binary. No invalidation, cleanup, GC or other
private WebKit operation is called; only lifecycle/mutation hooks and container
reads are performed. Allocation stacks and Style payload bytes are not collected.

The headless test loads the real libraries and installs all hooks, then checks
synthetic empty/deleted buckets, two keys/four entries, a 51-renderer queue,
address reuse receiving a fresh lifetime ID, invalid table metadata and excluded
foreign-thread objects. A request is acknowledged by the real system GLib timer,
on the same thread. JSONL counts and 0600 permissions are asserted. Preloading
`/bin/true` produces no extra output. Actual WebView lifecycle coverage still
requires checking all error counters during each measurement.

For an explicitly authorized run, use the printed hook path and an existing
private output directory. The ordinary launcher remains unchanged without the
observer environment variable. Close an existing instance normally first:

```bash
mkdir -p -m700 .debug/memory/owners
env -u Malloc \
  SMABAR_MEMORY_BINARY="$(realpath target/release/smabar)" \
  SMABAR_NATIVE_OBSERVER_HOOK=/absolute/path/to/build/webkit-native-observer.so \
  SMABAR_MEMORY_TRACE_DIR="$PWD/.debug/memory/owners" \
  scripts/dev.sh --no-watch --no-dev-server \
  --config '{"build":{"devUrl":null,"beforeDevCommand":""}}' \
  --runner "$PWD/scripts/memory/run-binary.sh"
```

`run-binary.sh` applies `LD_PRELOAD` only at the last launcher hop. It rejects
`Malloc` (including an empty value), existing preload/Heaptrack instrumentation,
loader path separators and missing paths. Only an actual executable basename
`WebKitWebProcess` activates the constructor. The trace directory must be visible
to the normal sandboxed child. Existing evidence is never overwritten: output is
exclusively created as `native-observer-<WebPID>.jsonl`, mode 0600. Failures terminate
the diagnostic WebProcess explicitly; do not enable this tool in ordinary use.

The default interval is 1000 ms; `SMABAR_NATIVE_OBSERVER_INTERVAL_MS` accepts 100–60000 ms.
An optional `native-observer-<WebPID>.request` file asks for a labeled snapshot:
write a temporary file with mode 0600 in the trace directory, then atomically rename
it to the request path. Contents are 1–64 bytes, including optional trailing newline;
the label allows only ASCII letters, digits, `_` and `-`. Permit only one pending
request. The next timer consumes it and writes an `event: snapshot` summary with
the same `label`; this summary is the ACK. Periodic snapshots use `periodic`.
Never interpret an unacknowledged boundary as a completed snapshot.

The timer resolves `g_timeout_add_full` from **system** `libglib-2.0.so.0`, avoiding
Gum's embedded GLib main context. Per-object cache/queue rows share `snapshot_id`
with the summary. `id` identifies a lifetime, not an address or DOM scope; address
reuse gets a new ID. Cache rows include the Resolver pointer, queue rows the
frame-view owner pointer. A Resolver can be shared by multiple shadow trees.
Compare IDs only within the same process.

The summary reports `cache_keys`, `cache_entries`, `queue_objects`, live counts,
constructor/destructor totals and hook calls. Require `reader_tid == main_tid`
and zero `lifecycle_errors`, `thread_errors`, `layout_errors`,
`discovered_without_ctor`, `foreign_owner_objects` and `destroying_objects` before
using a snapshot as complete. Foreign-thread or currently destroying objects are
excluded from reads. Nested reads during observed mutations are skipped. The fixed
registry supports 4096 concurrent objects; overflow is an explicit lifecycle error.

The private offsets audited for this exact ELF are:

| Container | Module-relative hook offsets |
| --- | --- |
| MatchedDeclarationsCache | ctor `0x3810660`, dtor `0x3810d20`, add `0x3811690`, sweep `0x3810760`, invalidate `0x3811c70`, remove `0x3811c00`, viewport clear `0x3811d60` |
| DetachedRendererList | append `0x301ebb0`, underlying SegmentedVector clear `0x2866360`, dtor `0x3021520` |
| Queue owner | LocalFrameViewLayoutContext ctor `0x301c870`, queue at context `+0x118`; its dtor calls the hooked vector dtor |

Cache `+0x10` is the table pointer. Table prefix uint32 fields `−0x0c`/`−0x04` are
key/slot counts. A 24-byte bucket contains key `+0`, entry-vector pointer `+8`,
capacity `+0x10`, size `+0x14`; keys 0/UINT32_MAX are empty/deleted. The walker checks
the header count against occupied buckets, at most 16384 keys and four entries per
key. Queue `+0` is size_t count, `+8` segment-pointer array, `+0x14` uint32 segment
count; each segment holds 50 pointers. The 5000 bound is per queue, so sums across
queues can exceed 5000. The observer reads queue metadata, not renderer pointers.

These limits come from the pinned [MatchedDeclarationsCache source](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebCore/style/MatchedDeclarationsCache.cpp)
and [DetachedRendererList source](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebCore/page/LocalFrameViewLayoutContext.cpp#L777).
Cache/queue counts are native retained objects, not retained bytes or a proof of
unbounded leakage. Sampled queue decreases provide only a lower bound on clears;
the global clear hook count includes calls on empty queues. Preserve complete
acknowledged JSONL prefixes alongside OS/PAS timestamps for offline comparison.
