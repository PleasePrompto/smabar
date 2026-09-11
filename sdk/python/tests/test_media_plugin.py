"""Contract tests for the bundled provider-driven media plugin."""

from __future__ import annotations

import importlib.util
import re
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


class MediaModule(Protocol):
    app: FakeApp
    state: dict[str, object]

    def on_media(self, data: dict[str, object]) -> None: ...

    def on_audio(self, data: dict[str, object]) -> None: ...

    def on_action(self, action: str, value: object) -> None: ...


def load_media() -> MediaModule:
    path = Path(__file__).resolve().parents[3] / "plugins" / "media" / "plugin.py"
    spec = importlib.util.spec_from_file_location("media_plugin_test", path)
    assert spec is not None and spec.loader is not None
    module: ModuleType = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    loaded = cast(MediaModule, module)
    loaded.app = FakeApp()
    loaded.state.clear()
    return loaded


def rendered(module: MediaModule, target: str) -> str:
    return next(
        html for _tile, item_target, html in reversed(module.app.rendered) if item_target == target
    )


def session(**overrides: object) -> dict[str, object]:
    value: dict[str, object] = {
        "id": "org.mpris.MediaPlayer2.demo",
        "identity": "Demo Player",
        "playbackState": "playing",
        "title": "Track",
        "artists": ["Artist"],
        "positionMs": 60_000,
        "durationMs": 240_000,
        "canControl": True,
        "canPlay": True,
        "canPause": True,
        "canGoNext": True,
        "canGoPrevious": True,
    }
    value.update(overrides)
    return value


def deliver(module: MediaModule, current: dict[str, object] | None) -> None:
    module.on_media(
        {
            "currentSessionId": current.get("id") if current is not None else None,
            "sessions": [current] if current is not None else [],
        }
    )


def button(html: str, action: str) -> str:
    match = re.search(rf'<button[^>]*data-action="{action}"[^>]*>', html)
    assert match is not None
    return match.group(0)


def test_idle_payload_renders_neutral_tile_and_clear_flyout() -> None:
    module = load_media()

    deliver(module, None)

    assert "media.no_session" in rendered(module, "tile")
    flyout = rendered(module, "flyout")
    assert "media.no_session" in flyout
    assert "media.no_session_hint" in flyout
    assert "data-action" not in flyout


def test_current_session_renders_escaped_metadata_status_and_progress() -> None:
    module = load_media()

    deliver(
        module,
        session(
            identity='<Demo & "Player">',
            title='<Track & "One">',
            artists=["A&B", "<Guest>"],
        ),
    )

    html = rendered(module, "tile") + rendered(module, "flyout")
    assert "&lt;Track &amp; &quot;One&quot;&gt;" in html
    assert "A&amp;B, &lt;Guest&gt;" in html
    assert "&lt;Demo &amp; &quot;Player&quot;&gt;" in html
    assert '<Track & "One">' not in html
    assert "media.state.playing" in html
    assert 'data-marquee style="max-width: 9rem"' in rendered(module, "tile")
    assert 'style="width: 25.0%"' in html
    assert "1:00 / 4:00" in html


def test_controls_follow_provider_capabilities() -> None:
    module = load_media()

    deliver(
        module,
        session(canGoPrevious=False, canPause=False, canGoNext=True),
    )

    html = rendered(module, "flyout")
    assert " disabled" in button(html, "previous")
    assert " disabled" in button(html, "playPause")
    assert " disabled" not in button(html, "next")


def test_known_actions_map_to_current_session_and_unknown_actions_are_ignored() -> None:
    module = load_media()
    current = session()
    deliver(module, current)

    for action in ("play", "pause", "playPause", "next", "previous", "unknown"):
        module.on_action(action, None)

    assert module.app.actions == [
        ("media", action, {"sessionId": current["id"]})
        for action in ("play", "pause", "playPause", "next", "previous")
    ]


def test_audio_controls_render_and_send_validated_provider_actions() -> None:
    module = load_media()
    module.on_audio(
        {
            "defaultOutput": {
                "name": '<Speakers & "Headphones">',
                "volumePercent": 37.4,
                "muted": False,
            }
        }
    )

    html = rendered(module, "flyout")
    assert "&lt;Speakers &amp; &quot;Headphones&quot;&gt;" in html
    assert 'type="range"' in html
    assert 'value="37"' in html
    assert 'data-action="setVolume"' in html
    assert 'data-field="volume"' in html
    assert 'data-action="setMuted"' in html
    assert 'data-value="true"' in html

    for action, value in [
        ("setVolume", "72"),
        ("setVolume", "101"),
        ("setVolume", "nope"),
        ("setMuted", "true"),
        ("setMuted", {}),
    ]:
        module.on_action(action, value)

    assert module.app.actions == [
        ("audio", "setVolume", {"volumePercent": 72}),
        ("audio", "setMuted", {"muted": True}),
    ]
