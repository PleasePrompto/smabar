#!/usr/bin/env python3
# Shared launch/quit helpers for the memory harness: start one app through
# scripts/dev.sh with the chosen binary, wait for six running plugins, quit
# through the tray menu over D-Bus.
import os
import re
import subprocess
import sys
import time
import urllib.error
from pathlib import Path

from mcp_client import Client

# Repository root; every launch goes through scripts/dev.sh.
ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).parent
sys.path.insert(0, str(ROOT / "scripts"))
from dev_processes import app_pids

# Measurement output (logs, PSS samples, captures); git-ignored by default.
OUT = Path(os.environ.get("SMABAR_MEMORY_OUT", ROOT / ".debug" / "memory"))
OUT.mkdir(parents=True, exist_ok=True)

def quit_app(pid):
    text = subprocess.check_output(['gdbus', 'call', '--session', '--dest',
        'org.kde.StatusNotifierWatcher', '--object-path', '/StatusNotifierWatcher',
        '--method', 'org.freedesktop.DBus.Properties.Get',
        'org.kde.StatusNotifierWatcher', 'RegisteredStatusNotifierItems'], text=True)
    item = next(value for value in re.findall(r"'([^']+)'", text) if f'_{pid}_' in value)
    dest, path = item.split('/', 1)
    args = ['gdbus', 'call', '--session', '--dest', dest, '--object-path', '/' + path + '/Menu', '--method']
    layout = subprocess.check_output(args + ['com.canonical.dbusmenu.GetLayout', '--', '0', '-1', '[]'], text=True)
    match = re.search(r"\((\d+), \{[^{}]*'label': <'(?:Quit|Beenden)'>", layout)
    if match is None: raise RuntimeError(f'Quit item missing: {layout}')
    subprocess.run(args + ['com.canonical.dbusmenu.Event', match[1], 'clicked', '<0>', '0'], check=True, capture_output=True)
    deadline = time.monotonic() + 20
    while Path(f'/proc/{pid}').exists():
        if time.monotonic() > deadline: raise RuntimeError(f'App {pid} did not quit')
        time.sleep(.2)

def start_app(binary, label, overrides=None):
    if app_pids(): raise RuntimeError('Existing smabar must be closed first')
    env = os.environ.copy()
    env.update(SMABAR_MEMORY_BINARY=str(binary), SMABAR_UV='/usr/lib/smabar/tools/uv',
               SMABAR_UPDATE_ENDPOINT='https://updates.smabar.com/latest.json')
    if overrides: env.update(overrides)
    with (OUT/f'{label}.log').open('w') as log:
        launcher = subprocess.Popen([str(ROOT/'scripts/dev.sh'), '--no-watch', '--no-dev-server',
            '--config', '{"build":{"devUrl":null,"beforeDevCommand":""}}',
            '--runner', str(HERE/'run-binary.sh')], env=env, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    deadline = time.monotonic() + 70
    while True:
        if launcher.poll() is not None: raise RuntimeError(f'Launcher exited: {label}.log')
        pids = app_pids()
        if len(pids) > 1: raise RuntimeError(f'Multiple apps: {pids}')
        try:
            if len(pids) == 1:
                plugins = Client().call('plugin_list')['structuredContent']['plugins']
                if len(plugins) == 6 and all(p['status'] == 'running' for p in plugins):
                    time.sleep(10)
                    return pids[0], launcher
        except (urllib.error.URLError, ConnectionResetError):
            pass # MCP is not listening yet during startup.
        if time.monotonic() > deadline: raise RuntimeError(f'Plugins did not start: {label}.log')
        time.sleep(.3)
