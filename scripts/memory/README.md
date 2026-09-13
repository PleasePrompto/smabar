# Memory harness (Linux)

Reproducible RAM windows against one smabar binary, launched through
`scripts/dev.sh` with the six-plugin user profile. Output lands in
`$SMABAR_MEMORY_OUT` (default `.debug/memory/`, git-ignored). Needs
`xdotool`, `gdbus`, Python 3, and for the inspector scripts `bun` plus a
devtools build started with `WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9228`.

| Script                 | Purpose                                                                                                  |
| ---------------------- | -------------------------------------------------------------------------------------------------------- |
| `cycle-run.py`         | One window: PSS/Anonymous every 5 s per group; `--cycle` opens a flyout every 10 s, `--smaps` mappings, `--native` captures, `--heaptrack` allocation trace of the app process |
| `inspect-run.py`       | Same cycle under a devtools build; `inspect-cycle.js` records WebKit memory categories and heap snapshots, `inspect-scripts.js` the evaluated JS programs and network requests |
| `quit-loop.py`         | Tray quits under gdb (`run-gdb.sh`); a hang leaves a thread dump                                          |
| `session.py`, `mcp_client.py`, `mcp-call.py`, `run-binary.sh` | Shared start/quit, MCP client, launcher hop that keeps `LD_PRELOAD` for heaptrack |

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
