"""Contract tests for systeminfo's provider-driven battery visibility."""

from __future__ import annotations

import importlib.util
import re
from html import unescape
from pathlib import Path
from types import ModuleType
from typing import Protocol, cast


class FakeApp:
    def __init__(self) -> None:
        self.rendered: list[tuple[str, str, str]] = []
        self.actions: list[tuple[str, str, dict[str, object]]] = []

    def t(self, key: str) -> str:
        return key

    def render(self, tile_id: str, target: str, html: str) -> None:
        self.rendered.append((tile_id, target, html))

    def provider_action(self, kind: str, action: str, **params: object) -> None:
        self.actions.append((kind, action, params))


class SystemInfoModule(Protocol):
    app: FakeApp
    state: dict[str, object]

    def on_battery(self, data: dict[str, object]) -> None: ...

    def on_cpu(self, data: dict[str, object]) -> None: ...

    def on_audio(self, data: dict[str, object]) -> None: ...

    def on_action(self, action: str, value: object) -> None: ...


def load_systeminfo() -> SystemInfoModule:
    path = Path(__file__).resolve().parents[3] / "plugins" / "systeminfo" / "plugin.py"
    spec = importlib.util.spec_from_file_location("systeminfo_battery_test", path)
    assert spec is not None and spec.loader is not None
    module: ModuleType = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return cast(SystemInfoModule, module)


def rendered_html(app: FakeApp) -> str:
    return "\n".join(html for _tile, _target, html in app.rendered)


def latest_tile_value(app: FakeApp) -> str:
    html = next(html for _tile, target, html in reversed(app.rendered) if target == "tile")
    match = re.search(r'<span class="sb-mono"[^>]*>([^<]*)</span>', html)
    assert match is not None
    return unescape(match.group(1))


def latest_tile_html(app: FakeApp) -> str:
    return next(html for _tile, target, html in reversed(app.rendered) if target == "tile")


def latest_hover_html(app: FakeApp) -> str:
    return next(html for _tile, target, html in reversed(app.rendered) if target == "hover")


def test_empty_battery_list_omits_all_battery_markup() -> None:
    module = load_systeminfo()
    module.app = FakeApp()
    module.state.clear()

    module.on_battery({"batteries": []})

    html = rendered_html(module.app)
    assert "systeminfo.battery" not in html
    assert 'data-lucide="battery"' not in html
    assert 'data-lucide="battery-charging"' not in html


def test_detected_battery_adds_tile_and_flyout_markup() -> None:
    module = load_systeminfo()
    module.app = FakeApp()
    module.state.clear()

    module.on_battery(
        {
            "batteries": [
                {
                    "chargePercent": 64.0,
                    "state": "charging",
                    "isCharging": True,
                }
            ]
        }
    )

    html = rendered_html(module.app)
    assert "systeminfo.battery" in html
    assert 'data-lucide="battery-charging"' in html
    assert "64%" in html


def test_cpu_percent_changes_keep_the_tile_value_width_stable() -> None:
    module = load_systeminfo()
    module.app = FakeApp()
    module.state.clear()

    htmls = []
    centers = []
    for percent in (5.0, 25.0, 100.0):
        module.on_cpu({"usagePercent": percent})
        htmls.append(latest_tile_html(module.app))
        centers.append(latest_tile_value(module.app))

    # progressRing cover: the ring carries the load as data-value and its
    # fixed size absorbs the digit-count change that used to resize the
    # tile; the center is a tweened bare number.
    assert 'data-value="5"' in htmls[0]
    assert 'data-value="25"' in htmls[1]
    assert 'data-value="100"' in htmls[2]
    assert centers == ["5", "25", "100"]
    # The memory line still pads to four monospace columns.
    memory = re.search(
        r'sb-tile-stack"><span class="sb-mono" data-sb-tween>([^<]*)</span>', htmls[0]
    )
    assert memory is not None
    assert len(memory.group(1)) == 4
    assert 'class="sb-kpi sb-center"' in rendered_html(module.app)


def test_cpu_updates_publish_a_stable_custom_hover_preview() -> None:
    module = load_systeminfo()
    module.app = FakeApp()
    module.state.clear()
    module.state["memory"] = {"usagePercent": 42.0}

    module.on_cpu({"usagePercent": 5.0})

    hover = latest_hover_html(module.app)
    assert "systeminfo.cpu" in hover
    assert "systeminfo.memory" in hover
    assert "5%" in hover
    assert "42%" in hover
    tile = latest_tile_html(module.app)
    assert 'title="' in tile


def test_audio_controls_render_and_send_validated_provider_actions() -> None:
    module = load_systeminfo()
    module.app = FakeApp()
    module.state.clear()

    module.on_audio(
        {
            "defaultOutput": {
                "name": "USB <Audio>",
                "volumePercent": 64.0,
                "muted": True,
            }
        }
    )

    html = rendered_html(module.app)
    assert "USB &lt;Audio&gt;" in html
    assert 'value="64"' in html
    assert 'aria-pressed="true"' in html
    assert 'data-action="setVolume"' in html
    assert 'data-field="volume"' in html

    module.on_action("setVolume", "25")
    module.on_action("setVolume", "-1")
    module.on_action("setMuted", "false")
    module.on_action("setMuted", [])
    assert module.app.actions == [
        ("audio", "setVolume", {"volumePercent": 25}),
        ("audio", "setMuted", {"muted": False}),
    ]
