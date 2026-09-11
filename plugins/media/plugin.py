# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Media controls backed by smabar's Core media provider."""

from html import escape

from smabar_sdk import Plugin

app = Plugin()
TILE = "media"
MEDIA_ACTIONS = frozenset({"play", "pause", "playPause", "next", "previous"})

state: dict[str, object] = {"currentSessionId": None, "sessions": [], "audio": None}


def _text(session: dict[str, object], key: str) -> str | None:
    value = session.get(key)
    return value.strip() if isinstance(value, str) and value.strip() else None


def _flag(session: dict[str, object], key: str) -> bool:
    return session.get(key) is True


def current_session() -> dict[str, object] | None:
    session_id = state.get("currentSessionId")
    sessions = state.get("sessions")
    if not isinstance(session_id, str) or not isinstance(sessions, list):
        return None
    return next(
        (
            session
            for session in sessions
            if isinstance(session, dict) and session.get("id") == session_id
        ),
        None,
    )


def playback_state(session: dict[str, object]) -> str:
    value = session.get("playbackState")
    return value if value in {"playing", "paused", "stopped"} else "stopped"


def title(session: dict[str, object]) -> str:
    return escape(_text(session, "title") or app.t("media.unknown_title"))


def artist_names(session: dict[str, object]) -> list[str]:
    raw = session.get("artists")
    if isinstance(raw, list):
        return [item.strip() for item in raw if isinstance(item, str) and item.strip()]
    return []


def artists(session: dict[str, object]) -> str:
    names = artist_names(session)
    return escape(", ".join(names) if names else app.t("media.unknown_artist"))


def milliseconds(session: dict[str, object], key: str) -> float | None:
    value = session.get(key)
    if isinstance(value, bool) or not isinstance(value, int | float):
        return None
    return max(0.0, float(value))


def format_time(value_ms: float) -> str:
    seconds = int(value_ms // 1000)
    hours, remainder = divmod(seconds, 3600)
    minutes, seconds = divmod(remainder, 60)
    return f"{hours}:{minutes:02d}:{seconds:02d}" if hours else f"{minutes}:{seconds:02d}"


def render_tile() -> None:
    session = current_session()
    if session is None:
        label = escape(app.t("media.no_session"))
        html = (
            f'<div class="sb-tile" title="{label}">'
            '<span data-lucide="music" aria-hidden="true"></span>'
            f'<span class="sb-muted">{label}</span></div>'
        )
    else:
        state_name = playback_state(session)
        tooltip = escape(
            f"{_text(session, 'title') or app.t('media.unknown_title')} — "
            f"{', '.join(artist_names(session)) or app.t('media.unknown_artist')} — "
            f"{app.t(f'media.state.{state_name}')}",
            quote=True,
        )
        # headlineMotion cover: the equalizer glyph IS the playback state —
        # animated while playing, frozen and dimmed otherwise. The tile HTML
        # is deliberately second-stable (no position in it): the equalizer is
        # a pure CSS loop, and a changed string would rebuild the DOM and
        # restart it every push.
        eq_class = "sb-eq" if state_name == "playing" else "sb-eq is-paused"
        html = (
            f'<div class="sb-tile" title="{tooltip}">'
            '<span class="sb-tile-stack">'
            f'<span data-marquee style="max-width: 9rem">{title(session)}</span>'
            f'<span class="sb-muted" data-marquee style="max-width: 9rem">'
            f"{artists(session)}</span></span>"
            f'<span class="{eq_class}" aria-hidden="true">'
            "<span></span><span></span><span></span><span></span><span></span>"
            "</span></div>"
        )
    app.render(TILE, "tile", html)


def progress(session: dict[str, object]) -> str:
    position = milliseconds(session, "positionMs")
    duration = milliseconds(session, "durationMs")
    if position is None:
        return ""
    if duration is None or duration <= 0:
        return f'<p class="sb-meta sb-mono">{format_time(position)}</p>'
    shown_position = min(position, duration)
    percent = shown_position / duration * 100
    label = escape(app.t("media.progress"))
    return (
        f'<div class="sb-progress" role="progressbar" aria-label="{label}"'
        f' aria-valuemin="0" aria-valuemax="100" aria-valuenow="{percent:.0f}">'
        f'<span style="width: {percent:.1f}%"></span></div>'
        f'<p class="sb-meta sb-mono">{format_time(shown_position)} / {format_time(duration)}</p>'
    )


def control(action: str, icon: str, label_key: str, enabled: bool) -> str:
    label = escape(app.t(label_key))
    disabled = "" if enabled else " disabled"
    return (
        f'<button class="sb-btn sb-btn-icon" type="button" data-action="{action}"'
        f' title="{label}" aria-label="{label}"{disabled}>'
        f'<span data-lucide="{icon}" aria-hidden="true"></span></button>'
    )


def controls(session: dict[str, object]) -> str:
    can_control = _flag(session, "canControl")
    state_name = playback_state(session)
    play_pause_enabled = can_control and _flag(
        session, "canPause" if state_name == "playing" else "canPlay"
    )
    play_pause_icon = "pause" if state_name == "playing" else "play"
    return (
        '<div class="sb-btn-group">'
        + control(
            "previous",
            "skip-back",
            "media.previous",
            can_control and _flag(session, "canGoPrevious"),
        )
        + control("playPause", play_pause_icon, "media.play_pause", play_pause_enabled)
        + control(
            "next",
            "skip-forward",
            "media.next",
            can_control and _flag(session, "canGoNext"),
        )
        + "</div>"
    )


def audio_controls() -> str:
    # Deliberately near-identical to systeminfo's audio_html(): plugins are
    # standalone single files by architecture (no cross-plugin imports, and
    # the SDK is an RPC contract, not a markup library), so the audio card
    # is duplicated on purpose. Change both together.
    payload = state.get("audio")
    if not isinstance(payload, dict):
        return ""
    heading = escape(app.t("media.volume"))
    output = payload.get("defaultOutput")
    if not isinstance(output, dict):
        return (
            f'<div class="sb-section">{heading}</div>'
            f'<p class="sb-muted">{escape(app.t("media.no_output"))}</p>'
        )
    raw_volume = output.get("volumePercent")
    volume = (
        max(0, min(100, round(float(raw_volume))))
        if isinstance(raw_volume, int | float) and not isinstance(raw_volume, bool)
        else 0
    )
    muted = output.get("muted") is True
    raw_name = output.get("name")
    name = raw_name.strip() if isinstance(raw_name, str) and raw_name.strip() else heading
    mute_label = escape(app.t("media.unmute" if muted else "media.mute"), quote=True)
    volume_label = escape(app.t("media.volume"), quote=True)
    icon = "volume-x" if muted or volume == 0 else "volume-1" if volume < 50 else "volume-2"
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
    heading = escape(app.t("media.title"))
    header = (
        '<div class="sb-header">'
        '<span class="sb-icon-badge"><span data-lucide="music" aria-hidden="true"></span></span>'
        f'<span class="sb-title">{heading}</span></div>'
    )
    session = current_session()
    if session is None:
        html = (
            header
            + f'<p class="sb-muted">{escape(app.t("media.no_session"))}</p>'
            + f'<p class="sb-meta">{escape(app.t("media.no_session_hint"))}</p>'
        )
    else:
        state_name = playback_state(session)
        identity = _text(session, "identity")
        identity_html = f'<p class="sb-meta">{escape(identity)}</p>' if identity else ""
        html = (
            header
            + '<div class="sb-card">'
            + f'<div class="sb-title">{title(session)}</div>'
            + f'<p class="sb-muted">{artists(session)}</p>'
            + f'<span class="sb-badge">{escape(app.t(f"media.state.{state_name}"))}</span>'
            + identity_html
            + progress(session)
            + "</div>"
            + controls(session)
        )
    html += audio_controls()
    app.render(TILE, "flyout", html)


def render_all() -> None:
    render_tile()
    render_flyout()


@app.on_ready
def on_ready() -> None:
    render_all()


@app.on_provider("media", interval_ms=1000)
def on_media(data: dict[str, object]) -> None:
    session_id = data.get("currentSessionId")
    sessions = data.get("sessions")
    state["currentSessionId"] = session_id if isinstance(session_id, str) else None
    state["sessions"] = sessions if isinstance(sessions, list) else []
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
        return
    session = current_session()
    if action not in MEDIA_ACTIONS or session is None:
        return
    session_id = session.get("id")
    if isinstance(session_id, str):
        app.provider_action("media", action, sessionId=session_id)


if __name__ == "__main__":
    app.run()
