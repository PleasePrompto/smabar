"""Typed clients for local audio and managed popup host services."""

from __future__ import annotations

from collections.abc import Callable
from typing import Literal, TypedDict

from smabar_sdk.protocol import RpcConnection


class AudioSource(TypedDict):
    root: Literal["plugin", "data"]
    path: str


type DesktopEventHandler = Callable[[dict[str, object]], None]


class Desktop:
    def __init__(self, conn: RpcConnection) -> None:
        self.conn = conn
        self.capabilities: frozenset[str] = frozenset()
        self.events: dict[str, list[DesktopEventHandler]] = {}

    def request(self, capability: str, method: str, params: dict[str, object]) -> dict[str, object]:
        if capability not in self.capabilities:
            raise RuntimeError(f"host does not advertise {capability}; upgrade smabar")
        result = self.conn.send_request(method, params)
        if not isinstance(result, dict):
            raise RuntimeError(f"invalid host response to {method}: expected an object")
        return result

    def on(self, event: str, handler: DesktopEventHandler) -> DesktopEventHandler:
        self.events.setdefault(event, []).append(handler)
        return handler


class Audio:
    def __init__(self, desktop: Desktop) -> None:
        self._desktop = desktop

    def play(
        self, playback_id: str, source: AudioSource, *, volume: int = 100, loop: bool = False
    ) -> dict[str, object]:
        """Play a local file; the same id replaces this plugin's previous playback."""
        return self._desktop.request(
            "audio",
            "audio.play",
            {"playbackId": playback_id, "source": source, "volume": volume, "loop": loop},
        )

    def _control(self, method: str, playback_id: str, **fields: object) -> dict[str, object]:
        return self._desktop.request("audio", method, {"playbackId": playback_id, **fields})

    def pause(self, playback_id: str) -> dict[str, object]:
        return self._control("audio.pause", playback_id)

    def resume(self, playback_id: str) -> dict[str, object]:
        return self._control("audio.resume", playback_id)

    def stop(self, playback_id: str) -> dict[str, object]:
        return self._control("audio.stop", playback_id)

    def set_volume(self, playback_id: str, volume: int) -> dict[str, object]:
        return self._control("audio.setVolume", playback_id, volume=volume)

    def seek(self, playback_id: str, position_ms: int) -> dict[str, object]:
        return self._control("audio.seek", playback_id, positionMs=position_ms)

    def status(self, playback_id: str) -> dict[str, object]:
        return self._control("audio.status", playback_id)

    def on_event(self, handler: DesktopEventHandler) -> DesktopEventHandler:
        return self._desktop.on("audio.event", handler)


class Popups:
    def __init__(self, desktop: Desktop) -> None:
        self._desktop = desktop

    def show(
        self,
        tile_id: str,
        popup_id: str,
        html: str,
        *,
        ttl_ms: int | None = None,
        sound: AudioSource | None = None,
    ) -> dict[str, object]:
        """Upsert a popup; acceptance does not mean it has appeared yet.

        on_event reports shown/dismissed/expired/dropped/suppressed. The sound
        plays once on first display; updating the same popup stays silent.
        """
        return self._desktop.request(
            "popups",
            "ui.popup.show",
            {
                "tileId": tile_id,
                "popupId": popup_id,
                "html": html,
                "ttlMs": ttl_ms,
                "sound": sound,
            },
        )

    def dismiss(self, tile_id: str, popup_id: str) -> dict[str, object]:
        return self._desktop.request(
            "popups", "ui.popup.dismiss", {"tileId": tile_id, "popupId": popup_id}
        )

    def on_event(self, handler: DesktopEventHandler) -> DesktopEventHandler:
        return self._desktop.on("ui.popup.event", handler)
