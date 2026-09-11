# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Template plugin: the size of one folder on this computer.

Small on purpose, complete on purpose — every piece a real plugin needs:
markup in views.py, strings in locales/, the first render from the cache in
on_ready, slow work on a thread, state in app.data_dir, settings read with a
fallback and written with set_settings, a popup, one action per control.
Copy it, rename it, replace the domain.

Optional app icon: put a 128x128 icon.png beside smabar.json for the settings
card (JPG/JPEG/ICO/WebP also work; transparency is optional).
Choose separately in smabar.json's tiles entry:
- usePluginIcon: true adds that image before this tile's cover HTML.
- iconSvg: "<svg ...>...</svg>" uses your own SVG instead; it wins if both are set.
- Omit both, or usePluginIcon: false without iconSvg, for no extra cover icon.
This template uses the last option; icons inside views.py remain part of its
chosen layout. Full file limits and examples: plugin_guide(section="manifest"),
folderIcon. Add real image bytes with filesystem/image tools; plugin_write_file
only writes UTF-8 text.
"""

import json
import os
import threading
import time

import views
from smabar_sdk import Plugin

app = Plugin()

TILE = "folder"
REFRESH_SECONDS = 600
LAYOUTS = ("progressRing", "statSplit", "basic")
CACHE_FILE = "measure.json"

# One dict for everything the tile shows. Handlers and the worker mutate
# it under the lock; render() reads a snapshot.
state: dict = {"used": 0, "files": 0, "measured_at": "", "error": "", "busy": False}
lock = threading.Lock()


def folder() -> str:
    return os.path.expanduser(str(app.settings.get("folder", "~/Downloads")))


def limit_bytes() -> int:
    """Configured limit in bytes; a bad value falls back instead of crashing."""
    try:
        return max(1, int(float(app.settings.get("limitGb", 5)) * 1e9))
    except (TypeError, ValueError):
        app.log("warn", "limitGb is not a number; using 5 GB")
        return 5 * 10**9


def layout() -> str:
    value = str(app.settings.get("coverLayout", LAYOUTS[0]))
    return value if value in LAYOUTS else LAYOUTS[0]


def load_cache() -> None:
    """The last measurement, so the tile shows a value before the first walk."""
    try:
        cached = json.loads((app.data_dir / CACHE_FILE).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return
    with lock:
        state.update(
            {
                key: cached[key]
                for key in ("used", "files", "measured_at")
                if key in cached
            }
        )


def save_cache() -> None:
    """Write beside the final name, then swap: a reader never sees half a file."""
    with lock:
        snapshot = {key: state[key] for key in ("used", "files", "measured_at")}
    path = app.data_dir / CACHE_FILE
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(snapshot), encoding="utf-8")
    os.replace(tmp, path)


def render() -> None:
    """Push every surface from the current state; render() is thread-safe."""
    with lock:
        snapshot = dict(state)
    app.render(
        TILE, "tile", views.tile(layout(), snapshot, limit_bytes(), folder(), app.t)
    )
    app.render(TILE, "hover", views.hover(snapshot, limit_bytes(), app.t))
    app.render(TILE, "flyout", views.flyout(snapshot, folder(), limit_bytes(), app.t))


def measure() -> None:
    """Worker thread: walks the folder, then updates, saves, renders, warns."""
    target = folder()
    used = files = 0
    error = ""
    if not os.path.isdir(target):
        error = views.tr(app.t, "folder.error", folder=target, reason="not a folder")
    for root, _dirs, names in os.walk(target):
        for name in names:
            try:
                used += os.path.getsize(os.path.join(root, name))
                files += 1
            except OSError:
                continue
    with lock:
        state.update({"busy": False, "error": error})
        if not error:
            now = time.strftime("%H:%M")
            state.update({"used": used, "files": files, "measured_at": now})
    if error:
        app.log("warn", "cannot measure the folder", folder=target)
    else:
        save_cache()
        if used > limit_bytes():
            html = views.popup(used, limit_bytes(), target, app.t)
            app.popups.show(TILE, "over-limit", html, ttl_ms=15000)
    render()


def refresh() -> None:
    """Starts one measurement; a second request while one runs is folded in."""
    with lock:
        if state["busy"]:
            return
        state["busy"] = True
    render()
    threading.Thread(target=measure, daemon=True).start()


@app.on_ready
def ready() -> None:
    """Runs once after initialize — settings and locales exist from here on."""
    load_cache()
    render()
    refresh()


@app.every(REFRESH_SECONDS)
def tick() -> None:
    # every() also fires right after initialize; the busy guard folds that
    # into the measurement on_ready started.
    refresh()


@app.on_action(TILE, "refresh")
def on_refresh(action: str, value: object) -> None:
    refresh()


@app.on_action(TILE, "save")
def on_save(action: str, value: object) -> None:
    """The form's fields arrive as a dict keyed by data-field."""
    fields = value if isinstance(value, dict) else {}
    try:
        limit = float(str(fields.get("limitGb", "")).replace(",", "."))
        if limit <= 0:
            raise ValueError(limit)
    except ValueError:
        with lock:
            state["error"] = app.t("folder.invalidLimit")
        render()
        return
    chosen = str(fields.get("folder", "")).strip() or folder()
    # set_settings REPLACES the whole settings object: spread the current ones.
    app.set_settings({**app.settings, "folder": chosen, "limitGb": limit})
    # The core answers with settings.changed; settings_changed() renders.


@app.on_settings_changed
def settings_changed(settings: dict) -> None:
    with lock:
        state["error"] = ""
    render()
    refresh()


if __name__ == "__main__":
    # Guarded so a test can import this module without starting the RPC loop.
    app.run()
