# Memory window with optional interaction: starts the given binary, samples
# PSS/Anonymous per process group every 5 s (scripts/measure-memory.py) and
# can open/close a flyout every 10 s (--cycle), sample the web process's
# mappings (--smaps), keep one tile (--tile), take native captures (--native)
# or trace allocations with heaptrack (--heaptrack). Usage:
#   cycle-run.py <binary> <label> [--probe MODE] [--cycle] [--tile ID]
#                [--minutes N] [--smaps] [--wiggle] [--native] [--heaptrack]
# Output: $SMABAR_MEMORY_OUT/<label>.jsonl (+ .log, PNGs, heaptrack/).
import argparse
import json
import re
import signal
import subprocess
import time
from pathlib import Path
from session import HERE, OUT, ROOT, app_pids, quit_app, start_app
from mcp_client import Client

parser = argparse.ArgumentParser()
parser.add_argument("binary", type=Path)
parser.add_argument("label")
parser.add_argument("--native", action="store_true")
parser.add_argument("--probe", choices=["observe", "no-events", "no-state", "no-dom"], default="observe")
# Synthetic desktop activity: the pointer alternates between two desktop
# points every second, away from the bar, to test the activity-ramp link.
parser.add_argument("--wiggle", action="store_true")
# Interaction: open and close a flyout every 10 s, cycling four tiles, to
# test whether flyout staging (native window, WebKit snapshot) steps memory up.
parser.add_argument("--cycle", action="store_true")
# PSS of the web process per mapping name every 30 s; the deltas say whether
# growth lands in anonymous heap, memfd, device nodes or libraries.
parser.add_argument("--smaps", action="store_true")
# One tile for every cycle: same overlay size each time. Growth that only
# appears with rotating tiles points at the resize, not at show/hide.
parser.add_argument("--tile")
# Allocation profile of the web process: heaptrack's preload library writes
# one raw file per process (`$$` = pid); the WebKitWebProcess file is
# interpreted after the window. Which WebKit code keeps memory, by call stack.
parser.add_argument("--heaptrack", action="store_true")
# Diagnostic windows are shorter; the 5-minute default stays for final A/B pairs.
parser.add_argument("--minutes", type=int, default=5)
args = parser.parse_args()
for current in app_pids():
    try:
        quit_app(current)
        print(f"Tray quit completed: {current}", flush=True)
    except RuntimeError as error:
        print(f"Known shutdown check failed: {error}; sending SIGTERM", flush=True)
        import os
        os.kill(current, signal.SIGTERM)
        deadline = time.monotonic()+10
        while Path(f"/proc/{current}").exists():
            if time.monotonic() > deadline:
                raise RuntimeError("SIGTERM did not stop the previous app")
            time.sleep(.2)
subprocess.run(["xdotool", "mousemove", "100", "500"], check=True)
overrides = {"SMABAR_MEMORY_PROBE": args.probe}
if args.heaptrack:
    heap_dir = OUT / "heaptrack"
    heap_dir.mkdir(exist_ok=True)
    overrides.update(SMABAR_HEAPTRACK_DIR=str(heap_dir))
pid, launcher = start_app(args.binary.resolve(), args.label, overrides)
print(json.dumps({"pid":pid,"binary":str(args.binary),"label":args.label,"started":time.time()}), flush=True)
Client().call("bar_ui_state", action="close_flyout")

def webkit_stats(root):
    # JSC_* environment of the web process proves inheritance; CPU seconds compare JIT arms.
    import os
    found = subprocess.run(["pgrep", "-P", str(root), "-f", "WebKitWebProcess"], capture_output=True, text=True).stdout.split()
    if not found:
        found = subprocess.run(["pgrep", "-f", "WebKitWebProcess"], capture_output=True, text=True).stdout.split()
    if not found:
        return None
    wp = found[0]
    env = [e for e in Path(f"/proc/{wp}/environ").read_bytes().decode(errors="replace").split("\0") if e.startswith(("JSC_", "WEBKIT_", "LIBGL_", "LD_PRELOAD", "DUMP_HEAPTRACK"))]
    fields = Path(f"/proc/{wp}/stat").read_text().rsplit(")", 1)[1].split()
    cpu = (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    # Shared-memory mappings: WebKit hands snapshot bitmaps over as memfd
    # regions; a count that grows per flyout cycle fingerprints kept bitmaps.
    maps = Path(f"/proc/{wp}/maps").read_text().splitlines()
    memfd = sum(1 for line in maps if "memfd:" in line)
    return {"pid": int(wp), "jscEnv": env, "cpuSeconds": cpu, "memfdMappings": memfd}

print(json.dumps({"webkitBefore": webkit_stats(pid)}), flush=True)
time.sleep(35)

# Desktop activity during the window: pointer position changes every 5 s
# (no idle-time source on Cinnamon/X11 here); ramps may follow activity.
import threading
pointer = {"moves": 0, "samples": 0, "stop": False}
def watch_pointer():
    last = None
    while not pointer["stop"]:
        position = subprocess.run(["xdotool", "getmouselocation"], capture_output=True, text=True).stdout.split(" screen")[0]
        pointer["samples"] += 1
        if last is not None and position != last:
            pointer["moves"] += 1
        last = position
        time.sleep(5)
watcher = threading.Thread(target=watch_pointer, daemon=True)
watcher.start()
def smaps_by_name(wp):
    totals, name = {}, None
    for line in Path(f"/proc/{wp}/smaps").read_text().splitlines():
        if re.match(r"^[0-9a-f]+-[0-9a-f]+ ", line):
            parts = line.split(None, 5)
            name = parts[5] if len(parts) > 5 else "[anon]"
            if "memfd:" in name: name = "memfd"
            elif name.startswith(("/usr/lib", "/lib", "/usr/share")): name = "libraries+data"
        elif line.startswith("Pss:") and name is not None:
            totals[name] = totals.get(name, 0) + int(line.split()[1])
    return totals
smaps_rows = []
def smaps_sampler(root):
    while not pointer["stop"]:
        stats = webkit_stats(root)
        if stats: smaps_rows.append(smaps_by_name(stats["pid"]))
        for _ in range(30):
            if pointer["stop"]: return
            time.sleep(1)
if args.smaps:
    threading.Thread(target=smaps_sampler, args=(pid,), daemon=True).start()
def wiggle():
    step = 0
    while not pointer["stop"]:
        x, y = (400, 500) if step % 2 == 0 else (1400, 700)
        subprocess.run(["xdotool", "mousemove", str(x), str(y)], check=False)
        step += 1
        time.sleep(1)
if args.wiggle:
    threading.Thread(target=wiggle, daemon=True).start()
def cycle():
    tiles = [args.tile] * 4 if args.tile else ["plugin:clock:clock", "plugin:systeminfo:system", "plugin:weather:weather", "plugin:todos:todos"]
    step = 0
    while not pointer["stop"]:
        try:
            Client().call("bar_ui_state", action="open_flyout", tileId=tiles[step % 4])
            time.sleep(2)
            Client().call("bar_ui_state", action="close_flyout")
        except Exception as error:  # noqa: BLE001 - diagnostic loop keeps going
            print(f"cycle error: {error}", flush=True)
        step += 1
        time.sleep(8)
if args.cycle:
    threading.Thread(target=cycle, daemon=True).start()
with (OUT/f"{args.label}.jsonl").open("w") as out:
    subprocess.run(["python3", str(ROOT/"scripts/measure-memory.py"), str(pid), "--interval", "5", "--samples", str(args.minutes * 12 + 1)], stdout=out, check=True)
pointer["stop"] = True
print("RAM sample completed", flush=True)
print(json.dumps({"pointer": {"moves": pointer["moves"], "samples": pointer["samples"]}}), flush=True)
print(json.dumps({"webkitAfter": webkit_stats(pid)}), flush=True)
if args.smaps and len(smaps_rows) >= 3:
    first, second, last = smaps_rows[0], smaps_rows[1], smaps_rows[-1]
    names = set(first) | set(last)
    deltas = sorted(((n, (second.get(n, 0) - first.get(n, 0)) / 1024, (last.get(n, 0) - second.get(n, 0)) / 1024) for n in names), key=lambda r: -abs(r[2]))
    print(json.dumps({"smapsDeltaMiB": [{"name": n, "firstWindow": round(a, 1), "rest": round(b, 1)} for n, a, b in deltas[:12]], "samples": len(smaps_rows)}), flush=True)
if args.heaptrack:
    # heaptrack finalizes on process exit; quit now and interpret the
    # web process file (name carries its pid).
    web = webkit_stats(pid)
    try:
        quit_app(pid)
    except RuntimeError as error:
        print(f"quit fallback: {error}", flush=True)
        import os
        os.kill(pid, signal.SIGTERM)
        time.sleep(5)
    raw = OUT / "heaptrack" / f"trace.{web['pid']}"
    interpreted = OUT / "heaptrack" / f"{args.label}.webkit.zst"
    with raw.open("rb") as source, interpreted.open("wb") as target:
        interpret = subprocess.Popen(["/usr/lib/heaptrack/libexec/heaptrack_interpret"], stdin=source, stdout=subprocess.PIPE)
        subprocess.run(["zstd", "-q", "-c"], stdin=interpret.stdout, stdout=target, check=True)
        interpret.wait()
    report = OUT / "heaptrack" / f"{args.label}.report.txt"
    with report.open("w") as out:
        subprocess.run(["heaptrack_print", "-n", "30", "-l", "-p", "-a", str(interpreted)], stdout=out, stderr=subprocess.STDOUT, check=False)
    print(json.dumps({"heaptrack": {"raw": str(raw), "rawBytes": raw.stat().st_size, "report": str(report)}}), flush=True)
if args.native:
    c=Client()
    c.screenshot("bar", OUT/f"{args.label}-bar.png")
    c.call("bar_ui_state", action="open_settings", group="system")
    c.screenshot("settings", OUT/f"{args.label}-settings.png")
    c.call("bar_ui_state", action="close_settings")
    c.screenshot("settings", OUT/f"{args.label}-settings-hidden.png")
    for tile in ["plugin:clock:clock", "plugin:systeminfo:system", "plugin:weather:weather", "plugin:todos:todos"]:
        for cycle in range(2):
            c.call("bar_ui_state", action="open_flyout", tileId=tile)
            c.screenshot("flyout", OUT/f"{args.label}-{tile.split(':')[1]}-{cycle}.png")
            c.call("bar_ui_state", action="close_flyout")
    c.call("bar_ui_state", action="open_settings", group="plugins/store")
    c.screenshot("settings", OUT/f"{args.label}-store.png")
    c.call("bar_ui_state", action="close_settings")
    print("Native captures and two cycles per flyout passed", flush=True)
