# Tray-quit reproduction: start the symbol binary under gdb, use flyout and
# settings, quit from the tray; on a hang send SIGUSR1 so gdb dumps all
# threads, then SIGTERM. Usage: quit-loop.py <binary> <rounds> [dwell-seconds]
# The failed quits of 12./13. September followed minutes of runtime; dwell
# keeps each round alive that long with periodic flyout use.
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from session import HERE, OUT, ROOT, app_pids, quit_app
from mcp_client import Client
import urllib.error

binary, rounds = Path(sys.argv[1]).resolve(), int(sys.argv[2])
dwell = int(sys.argv[3]) if len(sys.argv) > 3 else 0
runner = HERE / "run-gdb.sh"

def wait_gone(pid, seconds):
    deadline = time.monotonic() + seconds
    while Path(f"/proc/{pid}").exists() and time.monotonic() < deadline:
        time.sleep(0.2)
    return not Path(f"/proc/{pid}").exists()

def start(label):
    env = os.environ.copy()
    env.update(SMABAR_MEMORY_BINARY=str(binary), SMABAR_GDB_LOG=str(OUT / f"{label}.gdb.txt"),
               SMABAR_UV="/usr/lib/smabar/tools/uv", SMABAR_UPDATE_ENDPOINT="https://updates.smabar.com/latest.json")
    launcher = subprocess.Popen([str(ROOT / "scripts/dev.sh"), "--no-watch", "--no-dev-server",
        "--config", '{"build":{"devUrl":null,"beforeDevCommand":""}}', "--runner", str(runner)],
        env=env, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=open(OUT / f"{label}.log", "w"), stderr=subprocess.STDOUT, start_new_session=True)
    deadline = time.monotonic() + 90
    while True:
        if launcher.poll() is not None:
            raise RuntimeError(f"launcher exited: {label}.log")
        pids = app_pids()
        try:
            if len(pids) == 1:
                plugins = Client().call("plugin_list")["structuredContent"]["plugins"]
                if len(plugins) == 6 and all(p["status"] == "running" for p in plugins):
                    time.sleep(5)
                    return pids[0]
        except (urllib.error.URLError, ConnectionResetError):
            pass
        if time.monotonic() > deadline:
            raise RuntimeError(f"plugins did not start: {label}.log")
        time.sleep(0.3)

for current in app_pids():
    os.kill(current, signal.SIGTERM)
    wait_gone(current, 10)
results = []
for round_index in range(rounds):
    label = f"quit-{round_index:02d}"
    pid = start(label)
    client = Client()
    client.call("bar_ui_state", action="open_flyout", tileId="plugin:systeminfo:system")
    time.sleep(1)
    client.call("bar_ui_state", action="close_flyout")
    client.call("bar_ui_state", action="open_settings", group="system")
    time.sleep(1)
    client.call("bar_ui_state", action="close_settings")
    time.sleep(1)
    dwell_until = time.monotonic() + dwell
    while time.monotonic() < dwell_until:
        time.sleep(min(30, max(0.1, dwell_until - time.monotonic())))
        if time.monotonic() < dwell_until:
            client.call("bar_ui_state", action="open_flyout", tileId="plugin:clock:clock")
            time.sleep(1)
            client.call("bar_ui_state", action="close_flyout")
    started = time.monotonic()
    try:
        quit_app(pid)
        outcome = {"round": round_index, "pid": pid, "quitSeconds": round(time.monotonic() - started, 1), "hung": False}
    except RuntimeError as error:
        outcome = {"round": round_index, "pid": pid, "hung": True, "error": str(error)[:120]}
        if Path(f"/proc/{pid}").exists():
            os.kill(pid, signal.SIGUSR1)
            time.sleep(8)
            outcome["gdbLog"] = f"{label}.gdb.txt"
            if Path(f"/proc/{pid}").exists():
                os.kill(pid, signal.SIGTERM)
                wait_gone(pid, 10)
    for stray in app_pids():
        os.kill(stray, signal.SIGKILL)
    results.append(outcome)
    print(json.dumps(outcome), flush=True)
    if outcome["hung"]:
        break
    time.sleep(2)
(OUT / "quit-loop.json").write_text(json.dumps(results, indent=2))
