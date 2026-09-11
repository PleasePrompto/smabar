# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""systeminfo — the bundled reference plugin.

Shows live CPU / RAM and an optional battery in the tile, with CPU, memory,
disks, battery, and network in the flyout. All data comes from the smabar core
providers (no own sampling), all
visible text goes through app.t() with keys in locales/en.json (de.json is a
drop-in overlay), and all markup uses the smabar plugin UI kit (sb-* classes,
data-lucide icons, data-chart donuts) so the tile follows the active theme.
"""

from html import escape

from smabar_sdk import Plugin

NO_DATA = "–"

app = Plugin()
TILE = "system"

# Latest provider payloads, keyed by provider kind ("disk" holds the mount list).
state: dict[str, object] = {}


def _num(payload: object, key: str) -> float:
    if isinstance(payload, dict):
        value = payload.get(key)
        if isinstance(value, int | float):
            return float(value)
    return 0.0


def _pct(payload: object) -> float | None:
    """usagePercent of a provider payload, or None before the first sample."""
    if isinstance(payload, dict) and isinstance(
        payload.get("usagePercent"), int | float
    ):
        return _num(payload, "usagePercent")
    return None


def fmt_gb(size_bytes: float) -> str:
    return f"{size_bytes / 1024**3:.1f}"


def fmt_rate(bytes_per_sec: float) -> str:
    if bytes_per_sec >= 1024**2:
        return f"{bytes_per_sec / 1024**2:.1f} {app.t('systeminfo.unit.mbps')}"
    if bytes_per_sec >= 1024:
        return f"{bytes_per_sec / 1024:.0f} {app.t('systeminfo.unit.kbps')}"
    return f"{bytes_per_sec:.0f} {app.t('systeminfo.unit.bps')}"


def tile_percent(percent: float | None) -> str:
    """Four monospace columns, so changing digit count cannot resize the tile."""
    text = NO_DATA if percent is None else f"{percent:.0f}%"
    return text.rjust(4, "\N{NO-BREAK SPACE}")


def usage_class(percent: float) -> str:
    """Kit status class for a usage percentage — never a hardcoded color."""
    if percent >= 85:
        return "sb-warn"
    if percent >= 60:
        return ""
    return "sb-ok"


def battery_class(percent: float) -> str:
    if percent <= 15:
        return "sb-warn"
    if percent >= 40:
        return "sb-ok"
    return ""


def battery_payload() -> dict[str, object] | None:
    payload = state.get("battery")
    if isinstance(payload, dict) and isinstance(
        payload.get("chargePercent"), int | float
    ):
        return payload
    return None


def battery_state_text(payload: dict[str, object]) -> str:
    raw_state = payload.get("state")
    state_name = raw_state if isinstance(raw_state, str) else "unknown"
    if state_name not in {"charging", "discharging", "full", "empty"}:
        state_name = "unknown"
    return app.t(f"systeminfo.battery.state.{state_name}")


def gauge(percent: float, label: str, status_class: str | None = None) -> str:
    """One KPI cell: donut gauge (usage-colored) with the percent as center label."""
    pct = max(0.0, min(100.0, percent))
    css_class = usage_class(pct) if status_class is None else status_class
    return (
        f'<div class="sb-kpi sb-center">'
        f'<div class="sb-gauge {css_class}" data-chart="donut"'
        f' data-value="{pct:.0f}">'
        f'<span class="sb-mono sb-text-xs">{pct:.0f}%</span></div>'
        f'<span class="sb-kpi-label">{label}</span></div>'
    )


def list_row(icon: str, label: str, detail: str) -> str:
    return (
        f'<div class="sb-row"><span data-lucide="{icon}" aria-hidden="true"></span>'
        f"<span>{label}</span>"
        f'<span class="sb-muted sb-mono sb-push">{detail}</span></div>'
    )


def waiting() -> str:
    return f'<p class="sb-muted">{app.t("systeminfo.waiting")}</p>'


def header_html() -> str:
    return (
        '<div class="sb-header">'
        '<span class="sb-icon-badge"><span data-lucide="activity" aria-hidden="true"></span></span>'
        f'<span class="sb-title">{app.t("systeminfo.title")}</span></div>'
    )


def render_tile() -> None:
    cpu_pct = _pct(state.get("cpu"))
    mem_pct = _pct(state.get("memory"))
    cpu_text = tile_percent(cpu_pct)
    mem_text = tile_percent(mem_pct)
    title = (
        f"{app.t('systeminfo.cpu')} {cpu_text.strip()} · "
        f"{app.t('systeminfo.memory')} {mem_text.strip()}"
    )
    battery = battery_payload()
    battery_html = ""
    if battery is not None:
        battery_pct = _num(battery, "chargePercent")
        battery_icon = (
            "battery-charging" if battery.get("isCharging") is True else "battery"
        )
        title += f" · {app.t('systeminfo.battery')} {battery_pct:.0f}%"
        battery_html = (
            f'<span data-lucide="{battery_icon}" aria-hidden="true"'
            f' class="{battery_class(battery_pct)}"></span>'
            f'<span class="sb-mono">{tile_percent(battery_pct)}</span>'
        )
    # progressRing cover: the ring is CPU load (its color follows the usage
    # class via currentColor, the number carries data-sb-tween so changes
    # count instead of jumping — the ring fill glides shell-side); memory
    # sits beside it as a two-line stack. Units live in the title, the ring
    # center only fits ~3 characters.
    # Before the first sample the ring renders nothing (no data-value) and
    # the center shows the same placeholder the old tile used.
    cpu_value = "" if cpu_pct is None else f' data-value="{cpu_pct:.0f}"'
    cpu_center = NO_DATA if cpu_pct is None else f"{cpu_pct:.0f}"
    html = (
        f'<div class="sb-tile" title="{escape(title)}">'
        f'<span class="sb-gauge {usage_class(cpu_pct or 0.0)}"'
        f' data-chart="donut"{cpu_value}>'
        f'<span class="sb-mono" data-sb-tween>{cpu_center}</span></span>'
        '<span class="sb-tile-stack">'
        f'<span class="sb-mono" data-sb-tween>{mem_text}</span>'
        f'<span class="sb-muted">{escape(app.t("systeminfo.memory"))}</span></span>'
        f"{battery_html}</div>"
    )
    app.render(TILE, "tile", html)


def render_hover() -> None:
    rows = []
    cpu_pct = _pct(state.get("cpu"))
    if cpu_pct is not None:
        rows.append(list_row("cpu", app.t("systeminfo.cpu"), f"{cpu_pct:.0f}%"))
    mem_pct = _pct(state.get("memory"))
    if mem_pct is not None:
        rows.append(
            list_row("memory-stick", app.t("systeminfo.memory"), f"{mem_pct:.0f}%")
        )
    battery = battery_payload()
    if battery is not None:
        battery_pct = _num(battery, "chargePercent")
        icon = "battery-charging" if battery.get("isCharging") is True else "battery"
        rows.append(list_row(icon, app.t("systeminfo.battery"), f"{battery_pct:.0f}%"))
    body = f'<div class="sb-list">{"".join(rows)}</div>' if rows else waiting()
    app.render(TILE, "hover", header_html() + body)


def gauges_html() -> str:
    """CPU / RAM / per-mount donuts in one KPI grid; waiting before any sample."""
    cells = []
    cpu_pct = _pct(state.get("cpu"))
    if cpu_pct is not None:
        cells.append(gauge(cpu_pct, app.t("systeminfo.cpu")))
    mem_pct = _pct(state.get("memory"))
    if mem_pct is not None:
        cells.append(gauge(mem_pct, app.t("systeminfo.memory")))
    battery = battery_payload()
    if battery is not None:
        battery_pct = _num(battery, "chargePercent")
        cells.append(
            gauge(
                battery_pct,
                app.t("systeminfo.battery"),
                battery_class(battery_pct),
            )
        )
    mounts = state.get("disk")
    if isinstance(mounts, list):
        for mount in mounts:
            if isinstance(mount, dict):
                name = escape(str(mount.get("mountPoint", "?")))
                cells.append(gauge(_num(mount, "usagePercent"), name))
    if not cells:
        return waiting()
    return f'<div class="sb-kpi-grid">{"".join(cells)}</div>'


def details_html() -> str:
    rows = []
    cpu = state.get("cpu")
    if _pct(cpu) is not None:
        cores = int(_num(cpu, "coreCount"))
        ghz = _num(cpu, "frequencyMhz") / 1000.0
        detail = f"{cores} {app.t('systeminfo.cores')} · {ghz:.1f} {app.t('systeminfo.unit.ghz')}"
        rows.append(list_row("cpu", app.t("systeminfo.cpu"), detail))
    memory = state.get("memory")
    if _pct(memory) is not None:
        used, total = _num(memory, "usedBytes"), _num(memory, "totalBytes")
        detail = f"{fmt_gb(used)} / {fmt_gb(total)} {app.t('systeminfo.unit.gb')}"
        rows.append(list_row("memory-stick", app.t("systeminfo.memory"), detail))
    battery = battery_payload()
    if battery is not None:
        battery_pct = _num(battery, "chargePercent")
        battery_icon = (
            "battery-charging" if battery.get("isCharging") is True else "battery"
        )
        detail = f"{battery_pct:.0f}% · {battery_state_text(battery)}"
        rows.append(list_row(battery_icon, app.t("systeminfo.battery"), detail))
    mounts = state.get("disk")
    if isinstance(mounts, list):
        unit = app.t("systeminfo.unit.gb")
        for mount in mounts:
            if not isinstance(mount, dict):
                continue
            used, total = _num(mount, "usedBytes"), _num(mount, "totalBytes")
            name = escape(str(mount.get("mountPoint", "?")))
            rows.append(
                list_row("hard-drive", name, f"{fmt_gb(used)} / {fmt_gb(total)} {unit}")
            )
    if not rows:
        return waiting()
    return f'<div class="sb-list">{"".join(rows)}</div>'


def network_html() -> str:
    network = state.get("network")
    if not isinstance(network, dict):
        return waiting()
    rx = fmt_rate(_num(network, "rxBytesPerSec"))
    tx = fmt_rate(_num(network, "txBytesPerSec"))
    return (
        '<div class="sb-list">'
        + list_row("arrow-down", app.t("systeminfo.down"), rx)
        + list_row("arrow-up", app.t("systeminfo.up"), tx)
        + "</div>"
    )


def audio_html() -> str:
    # Deliberately near-identical to media's audio_controls(): plugins are
    # standalone single files by architecture (no cross-plugin imports, and
    # the SDK is an RPC contract, not a markup library), so the audio card
    # is duplicated on purpose. Change both together.
    payload = state.get("audio")
    if not isinstance(payload, dict):
        return ""
    heading = escape(app.t("systeminfo.audio"))
    output = payload.get("defaultOutput")
    if not isinstance(output, dict):
        return (
            f'<div class="sb-section">{heading}</div>'
            f'<p class="sb-muted">{escape(app.t("systeminfo.audio.no_output"))}</p>'
        )
    raw_volume = output.get("volumePercent")
    volume = (
        max(0, min(100, round(float(raw_volume))))
        if isinstance(raw_volume, int | float) and not isinstance(raw_volume, bool)
        else 0
    )
    muted = output.get("muted") is True
    raw_name = output.get("name")
    name = (
        raw_name.strip() if isinstance(raw_name, str) and raw_name.strip() else heading
    )
    mute_label = escape(
        app.t("systeminfo.audio.unmute" if muted else "systeminfo.audio.mute"),
        quote=True,
    )
    volume_label = escape(app.t("systeminfo.audio.volume"), quote=True)
    icon = (
        "volume-x"
        if muted or volume == 0
        else "volume-1"
        if volume < 50
        else "volume-2"
    )
    return (
        f'<div class="sb-section">{heading}</div>'
        '<div class="sb-card">'
        '<div class="sb-row">'
        f'<button class="sb-btn sb-btn-icon" type="button" data-action="setMuted"'
        f' data-value="{str(not muted).lower()}"'
        f' title="{mute_label}" aria-label="{mute_label}" aria-pressed="{str(muted).lower()}">'
        f'<span data-lucide="{icon}" aria-hidden="true"></span></button>'
        f'<span class="sb-muted">{escape(name)}</span>'
        f'<span class="sb-mono sb-push">{volume}%</span></div>'
        '<div class="sb-range-wrap" data-sb-range data-sb-range-unit="%">'
        f'<input class="sb-range" type="range" min="0" max="100" step="1" value="{volume}"'
        f' data-action="setVolume" data-field="volume" aria-label="{volume_label}">'
        '<output class="sb-range-wrap__bubble"></output></div></div>'
    )


def render_flyout() -> None:
    html = (
        header_html()
        + gauges_html()
        + f'<div class="sb-section">{app.t("systeminfo.details")}</div>'
        + details_html()
        + audio_html()
        + f'<div class="sb-section">{app.t("systeminfo.network")}</div>'
        + network_html()
    )
    app.render(TILE, "flyout", html)


def render_all() -> None:
    render_tile()
    render_hover()
    render_flyout()


@app.on_provider("cpu", interval_ms=1000)
def on_cpu(data: dict[str, object]) -> None:
    state["cpu"] = data
    render_all()


@app.on_provider("memory", interval_ms=1000)
def on_memory(data: dict[str, object]) -> None:
    state["memory"] = data
    render_all()


@app.on_provider("disk", interval_ms=5000)
def on_disk(data: dict[str, object]) -> None:
    # The disk payload is a top-level array; the SDK wraps it as {"value": [...]}.
    mounts = data.get("value")
    state["disk"] = mounts if isinstance(mounts, list) else []
    render_flyout()


@app.on_provider("network", interval_ms=1000)
def on_network(data: dict[str, object]) -> None:
    state["network"] = data
    render_flyout()


@app.on_provider("battery")
def on_battery(data: dict[str, object]) -> None:
    batteries = data.get("batteries")
    first = batteries[0] if isinstance(batteries, list) and batteries else None
    state["battery"] = (
        first
        if isinstance(first, dict)
        and isinstance(first.get("chargePercent"), int | float)
        else None
    )
    render_all()


@app.on_provider("audio")
def on_audio(data: dict[str, object]) -> None:
    state["audio"] = data
    render_flyout()


@app.on_action(TILE)
def on_action(action: str, value: object) -> None:
    if action == "setMuted" and isinstance(value, str) and value in {"true", "false"}:
        app.provider_action("audio", "setMuted", muted=value == "true")
        return
    if action == "setVolume" and isinstance(value, str) and value.isdigit():
        volume = int(value)
        if 0 <= volume <= 100:
            app.provider_action("audio", "setVolume", volumePercent=volume)


if __name__ == "__main__":
    app.run()
