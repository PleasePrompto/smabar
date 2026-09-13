# Flyout cycles under a devtools build with WebKit's inspector HTTP server:
# runs inspect-cycle.js (memory categories, heap snapshots) or
# inspect-scripts.js (evaluated programs and requests) next to the PSS sampler.
# Usage: inspect-run.py <devtools-binary> <label> [cycles] [inspector script]
import json, os, subprocess, sys
from session import HERE, OUT, ROOT, app_pids, quit_app, start_app
binary = sys.argv[1]
label = sys.argv[2]
cycles = sys.argv[3] if len(sys.argv) > 3 else "12"
script = sys.argv[4] if len(sys.argv) > 4 else "inspect-cycle.js"
for current in app_pids():
    quit_app(current)
pid, launcher = start_app(binary, label, {"WEBKIT_INSPECTOR_HTTP_SERVER": "127.0.0.1:9228"})
print(json.dumps({"pid": pid, "label": label}), flush=True)
with (OUT / f"{label}.jsonl").open("w") as out:
    sampler = subprocess.Popen([sys.executable, str(ROOT / "scripts/measure-memory.py"), str(pid), "--interval", "5", "--samples", str(int(cycles) * 2 + 8)], stdout=out)
    subprocess.run(["bun", str(HERE / script), label, cycles], cwd=HERE, env={**os.environ, "SMABAR_MEMORY_OUT": str(OUT)}, check=True)
    sampler.wait()
rows = [json.loads(line) for line in (OUT / f"{label}.jsonl").read_text().splitlines() if line.strip()]
pss = [r["groups"]["webview"]["Pss"] / 1024 for r in rows]
print(f"webview PSS every 30 s {[round(v) for v in pss[::6]]} growth {pss[-1] - pss[0]:+.1f} MiB", flush=True)
