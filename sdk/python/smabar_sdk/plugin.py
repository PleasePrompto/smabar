"""The Plugin class: single-file plugin DX on top of the smabar RPC protocol."""

from __future__ import annotations

import contextlib
import copy
import sys
import threading
import traceback
from collections.abc import Callable
from pathlib import Path
from typing import Literal

from smabar_sdk._handlers import SerializedHandlers
from smabar_sdk._locales import is_language_code, load_plugin_locales
from smabar_sdk.commands import Commands
from smabar_sdk.desktop import Audio, Desktop, Popups
from smabar_sdk.protocol import (
    INTERNAL_ERROR,
    INVALID_REQUEST,
    METHOD_NOT_FOUND,
    Reader,
    RpcConnection,
    RpcError,
    Writer,
)

type LogLevel = Literal["debug", "info", "warn", "error"]
type ActionHandler = Callable[[str, object], None]
type TickHandler = Callable[[], None]
type ProviderHandler = Callable[[dict[str, object]], None]
type SettingsHandler = Callable[[dict[str, object]], None]

PROVIDER_INTERVAL_MIN_MS = 250
PROVIDER_INTERVAL_MAX_MS = 3_600_000


class Plugin:
    """A smabar plugin: register handlers with decorators, then call run().

    All user handlers run serially on one worker. The dedicated RPC reader stays
    free to receive host responses while a handler makes a blocking host call.
    """

    def __init__(self, reader: Reader | None = None, writer: Writer | None = None) -> None:
        self._conn = RpcConnection(
            reader if reader is not None else sys.stdin,
            writer if writer is not None else sys.stdout,
        )
        self._stop = threading.Event()
        self._handlers = SerializedHandlers(self._stop, self._report_handler_error)
        self._commands = Commands(self._conn, self._handlers, self._report_handler_error)
        self.command = self._commands.register
        self._desktop = Desktop(self._conn)
        self.audio = Audio(self._desktop)
        self.popups = Popups(self._desktop)
        self._action_handlers: list[tuple[str, str | None, ActionHandler]] = []
        self._tick_handlers: list[tuple[float, TickHandler]] = []
        self._provider_handlers: dict[str, tuple[int, ProviderHandler]] = {}
        self._available_providers: set[str] = set()
        self._warned_unavailable_providers: set[str] = set()
        self._settings_handlers: list[SettingsHandler] = []
        self._ready_handlers: list[TickHandler] = []
        self._initialized = False
        self._warned_early = False
        self._timer_threads: list[threading.Thread] = []
        self._timers_started = False
        self._plugin_id: str | None = None
        self._data_dir: Path | None = None
        self._plugin_dir: Path | None = None
        self._settings: dict[str, object] = {}
        self._language = "en"
        self._locale: dict[str, str] = {}
        self._plugin_locale: dict[str, str] = {}

    @property
    def capabilities(self) -> frozenset[str]:
        """Desktop host capabilities advertised at initialize; empty on old hosts."""
        return self._desktop.capabilities

    @property
    def plugin_id(self) -> str:
        """The plugin id assigned by the core; available after 'initialize'."""
        if self._plugin_id is None:
            raise RuntimeError("plugin_id is only available after the core sent 'initialize'")
        return self._plugin_id

    @property
    def data_dir(self) -> Path:
        """Writable directory this plugin owns; available after 'initialize'.

        Writing here never restarts the plugin, and the directory is removed
        together with the plugin. This is where caches, state and downloads go.
        """
        if self._data_dir is None:
            raise RuntimeError("data_dir is only available after the core sent 'initialize'")
        return self._data_dir

    @property
    def plugin_dir(self) -> Path:
        """The plugin's own code folder; read-only, available after 'initialize'.

        Bundled files (`locales/`, templates, …) live here. Writing into it
        restarts the plugin — use `data_dir` for anything you produce.
        """
        if self._plugin_dir is None:
            raise RuntimeError("plugin_dir is only available after the core sent 'initialize'")
        return self._plugin_dir

    @property
    def settings(self) -> dict[str, object]:
        """Current plugin settings, kept up to date via 'settings.changed'.

        Read-only by contract: mutating the returned dict does NOT reach the
        core — use `set_settings()` to persist changes.
        """
        self._warn_if_early("settings")
        return copy.deepcopy(self._settings)

    @property
    def language(self) -> str:
        """The app's UI language code (e.g. "en", "de"); default "en" before 'initialize'.

        The core delivers the code only in 'initialize' — a language change at
        runtime does NOT reach a running plugin yet; the plugin picks the new
        language up on its next restart.
        """
        return self._language

    @property
    def locale(self) -> dict[str, str]:
        """Locale strings (key → translated text) handed over by the core.

        Read-only by contract: mutating the returned dict does not alter the
        SDK's own lookup state.
        """
        return self._locale.copy()

    def t(self, key: str) -> str:
        """Look up a translated string for `key`.

        Lookup order: the plugin's own locales (`<plugin-dir>/locales/en.json`
        overlaid with `locales/<language>.json`, loaded on 'initialize'), then
        the app locale map handed over by the core, then the key itself.
        """
        self._warn_if_early("t()")
        if key in self._plugin_locale:
            return self._plugin_locale[key]
        return self._locale.get(key, key)

    def render(self, tile_id: str, target: str, html: str, ttl_ms: int | None = None) -> None:
        """Render HTML into a tile target: "tile", "flyout", "hover", or "popup".

        "hover" is an optional hover-preview: once pushed, hovering the tile
        shows this content instead of the click flyout (even when the user
        disabled the generic hover preview); a click still opens the normal
        "flyout" content. Put actions into the flyout header (sb-header).

        Popups stack at a user-configured screen position. ttl_ms=None makes
        a popup STICKY (it stays until the user dismisses it); a number
        auto-dismisses after that many ms, clamped by the shell to
        1000-120000. Up to 5 are visible and 45 wait; when that bounded queue
        is full, the oldest waiting popup is discarded.
        """
        self._warn_if_early("render()")
        params: dict[str, object] = {"tileId": tile_id, "target": target, "html": html}
        if ttl_ms is not None:
            params["ttlMs"] = ttl_ms
        self._conn.send_notification("ui.render", params)

    def _warn_if_early(self, what: str) -> None:
        """Warn once when plugin data is used before the core sent 'initialize'.

        Before that moment there are no locales and no settings, so t() returns
        raw keys and settings.get() returns nothing — a plugin that renders at
        import time ships untranslated markup and default values, silently and
        forever. This turns that into one log line.
        """
        if self._initialized or self._warned_early:
            return
        self._warned_early = True
        self.log(
            "warn",
            f"{what} used before 'initialize': locales and settings are still empty. "
            "Do your first render from @app.on_ready (or @app.every), not at import time.",
        )

    def log(self, level: LogLevel, message: str, **fields: object) -> None:
        """Send a structured log line to the core (ends up in the plugin log)."""
        params: dict[str, object] = {"level": level, "message": message}
        if fields:
            params["fields"] = fields
        self._conn.send_notification("log", params)

    def get_settings(self) -> dict[str, object]:
        """Fetch the current settings from the core (blocking request).

        The dedicated reader resolves the response while this handler waits.
        """
        result = self._conn.send_request("settings.get", {})
        settings = result.get("settings") if isinstance(result, dict) else None
        return settings if isinstance(settings, dict) else {}

    def set_settings(self, settings: dict[str, object]) -> None:
        """Persist new settings in the core (blocking request).

        Like `get_settings`, this works from handlers and other worker threads.
        """
        self._conn.send_request("settings.set", {"settings": settings})
        self._settings = dict(settings)

    def provider_action(self, kind: str, action: str, **args: object) -> None:
        """Request a provider action (blocking).

        Media supports ``play``, ``pause``, ``playPause``, ``next`` and
        ``previous``. Pass ``sessionId=<id>`` to target one advertised media
        session, or omit it to use the provider's current session. Audio
        supports ``setVolume`` with ``volumePercent=0..100`` and ``setMuted``
        with ``muted=<bool>``.
        """
        params: dict[str, object] = {"kind": kind, "action": action}
        params.update(args)
        self._conn.send_request("provider.action", params)

    def on_action(
        self, tile_id: str, action: str | None = None
    ) -> Callable[[ActionHandler], ActionHandler]:
        """Register a handler for UI events of one tile; action=None matches all."""

        def register(fn: ActionHandler) -> ActionHandler:
            self._action_handlers.append((tile_id, action, fn))
            return fn

        return register

    def every(self, seconds: float) -> Callable[[TickHandler], TickHandler]:
        """Run a handler periodically, starting right after 'initialize'."""
        if seconds <= 0:
            raise ValueError("every() needs a positive interval in seconds")

        def register(fn: TickHandler) -> TickHandler:
            self._tick_handlers.append((seconds, fn))
            return fn

        return register

    def on_provider(
        self, kind: str, interval_ms: int = 0
    ) -> Callable[[ProviderHandler], ProviderHandler]:
        """Subscribe to one advertised provider during initialization.

        ``interval_ms=0`` uses the provider default; explicit intervals must
        be between 250 ms and one hour.
        """
        if (
            isinstance(interval_ms, bool)
            or not isinstance(interval_ms, int)
            or (
                interval_ms != 0
                and not PROVIDER_INTERVAL_MIN_MS <= interval_ms <= PROVIDER_INTERVAL_MAX_MS
            )
        ):
            raise ValueError(
                "on_provider() interval_ms must be 0 or an integer from "
                f"{PROVIDER_INTERVAL_MIN_MS} to {PROVIDER_INTERVAL_MAX_MS}"
            )

        def register(fn: ProviderHandler) -> ProviderHandler:
            self._provider_handlers[kind] = (interval_ms, fn)
            return fn

        return register

    def on_ready(self, fn: TickHandler) -> TickHandler:
        """Register a handler that runs once, right after 'initialize'.

        This is where the FIRST render belongs: it is the earliest point at
        which the plugin's locales, its settings and its plugin id exist.
        """
        self._ready_handlers.append(fn)
        return fn

    def on_settings_changed(self, fn: SettingsHandler) -> SettingsHandler:
        """Register a handler that receives the new settings dict on every change."""
        self._settings_handlers.append(fn)
        return fn

    def run(self) -> None:
        """Serve the RPC loop until the core sends 'shutdown' or closes stdin."""
        self._handlers.start()
        try:
            while True:
                message = self._conn.read_message()
                if message is None:
                    break
                if self._dispatch(message):
                    break
        finally:
            self._conn.fail_pending("plugin is shutting down")
            self._stop_handlers()

    def _dispatch(self, message: dict[str, object]) -> bool:
        """Handle one incoming request or notification; True means stop the loop."""
        method = message.get("method")
        raw_params = message.get("params")
        params: dict[str, object] = raw_params if isinstance(raw_params, dict) else {}
        request_id = message.get("id")
        if not isinstance(method, str):
            if request_id is not None:
                self._conn.send_error(request_id, INVALID_REQUEST, "message has no method")
            return False
        if request_id is not None:
            return self._handle_request(request_id, method, params)
        self._handle_notification(method, params)
        return False

    def _handle_request(self, request_id: object, method: str, params: dict[str, object]) -> bool:
        # A raising request handler must answer with a JSON-RPC error, not
        # die silently: the core would otherwise see EOF and restart the
        # plugin, turning one bad request into a crash loop.
        try:
            return self._dispatch_request(request_id, method, params)
        except Exception as exc:
            self._conn.send_error(request_id, INTERNAL_ERROR, f"{type(exc).__name__}: {exc}")
            # 'initialize' failed: the plugin is not in a usable state, so
            # stop the loop instead of serving on half-initialized state.
            return method == "initialize"

    def _dispatch_request(self, request_id: object, method: str, params: dict[str, object]) -> bool:
        if method == "initialize":
            self._apply_initialize(params)
            self._initialized = True
            commands = self._commands.describe()
            self._conn.send_result(request_id, {"commands": commands} if commands else {})
            if not self._timers_started:
                self._timers_started = True
                try:
                    self._subscribe_providers()
                    for handler in self._ready_handlers:
                        self._run_handler("on_ready", handler, wait=True)
                    self._start_timers()
                except Exception as exc:  # initialize already has its one response
                    message = "initialize setup failed; core will restart the plugin"
                    with contextlib.suppress(Exception):  # logger must not trigger another response
                        self.log("error", message, error=f"{type(exc).__name__}: {exc}")
                    return True
            return False
        if method == "command.call":
            self._commands.dispatch(request_id, params)
            return False
        if method in ("ping", "shutdown"):
            self._conn.send_result(request_id, {})
            return method == "shutdown"
        self._conn.send_error(request_id, METHOD_NOT_FOUND, f"unknown method {method!r}")
        return False

    def _handle_notification(self, method: str, params: dict[str, object]) -> None:
        for handler in self._desktop.events.get(method, []):
            self._run_handler(method, handler, params)
        if method == "event":
            tile_id = params.get("tileId")
            action = params.get("action")
            if isinstance(tile_id, str) and isinstance(action, str):
                self._dispatch_action(tile_id, action, params.get("value"))
        elif method == "provider.data":
            kind = params.get("kind")
            entry = self._provider_handlers.get(kind) if isinstance(kind, str) else None
            if entry is not None:
                data = params.get("data")
                # Handlers always get a dict; scalar provider data is wrapped.
                payload = data if isinstance(data, dict) else {"value": data}
                self._run_handler(f"on_provider({kind!r})", entry[1], payload)
        elif method == "settings.changed":
            settings = params.get("settings")
            if isinstance(settings, dict):
                self._run_handler("on_settings_changed", self._apply_settings, settings)
        # Unknown notifications are ignored; the core logs raw traffic anyway.

    def _dispatch_action(self, tile_id: str, action: str, value: object) -> None:
        for handler_tile, handler_action, fn in self._action_handlers:
            if handler_tile == tile_id and handler_action in (None, action):
                self._run_handler(f"on_action({tile_id!r})", fn, action, value)

    def _apply_initialize(self, params: dict[str, object]) -> None:
        capabilities = params.get("capabilities", [])
        if not isinstance(capabilities, list) or not all(isinstance(x, str) for x in capabilities):
            raise ValueError("initialize.capabilities must be a list of strings")
        self._desktop.capabilities = frozenset(capabilities)
        plugin_id = params.get("pluginId")
        self._plugin_id = plugin_id if isinstance(plugin_id, str) else ""
        raw_data_dir = params.get("dataDir")
        self._data_dir = Path(raw_data_dir) if isinstance(raw_data_dir, str) else Path()
        raw_plugin_dir = params.get("pluginDir")
        if not isinstance(raw_plugin_dir, str) or raw_plugin_dir == "":
            raise ValueError("initialize.pluginDir must be a non-empty string")
        self._plugin_dir = Path(raw_plugin_dir)
        settings = params.get("settings")
        if isinstance(settings, dict):
            self._settings = settings
        language = params.get("language")
        self._language = (
            language if isinstance(language, str) and is_language_code(language) else "en"
        )
        locale = params.get("locale")
        if isinstance(locale, dict):
            self._locale = {
                key: value
                for key, value in locale.items()
                if isinstance(key, str) and isinstance(value, str)
            }
        providers = params.get("providers")
        if not isinstance(providers, list) or not all(
            isinstance(provider, str) for provider in providers
        ):
            raise ValueError("initialize.providers must be a list of strings")
        self._available_providers = set(providers)
        self._plugin_locale = load_plugin_locales(
            self._plugin_dir,
            self._language,
            lambda message: self.log("warn", message),
        )

    def _subscribe_providers(self) -> None:
        for kind, (interval_ms, _fn) in self._provider_handlers.items():
            if kind not in self._available_providers:
                if kind not in self._warned_unavailable_providers:
                    self._warned_unavailable_providers.add(kind)
                    self.log(
                        "warn",
                        f"provider {kind!r} is not available on this platform; its "
                        "@app.on_provider handler was not subscribed. Render an empty or "
                        "unavailable state from @app.on_ready instead.",
                    )
                continue
            params: dict[str, object] = {"kind": kind}
            if interval_ms > 0:
                params["intervalMs"] = interval_ms
            try:
                self._conn.send_request("provider.subscribe", params)
            except RpcError as exc:
                self.log("error", f"provider.subscribe for {kind!r} failed: {exc}")

    def _start_timers(self) -> None:
        for seconds, fn in self._tick_handlers:
            thread = threading.Thread(
                target=self._timer_loop,
                args=(seconds, fn),
                name=f"smabar-every-{seconds:g}s",
                daemon=True,
            )
            thread.start()
            self._timer_threads.append(thread)

    def _timer_loop(self, seconds: float, fn: TickHandler) -> None:
        while not self._stop.is_set():
            self._run_handler(f"every({seconds:g})", fn, wait=True)
            if self._stop.wait(seconds):
                return

    def _stop_handlers(self) -> None:
        self._stop.set()
        for thread in self._timer_threads:
            thread.join(timeout=1.0)
        self._handlers.stop()

    def _run_handler(
        self,
        origin: str,
        fn: Callable[..., None],
        *args: object,
        wait: bool = False,
    ) -> None:
        self._handlers.submit(origin, fn, *args, wait=wait)

    def _apply_settings(self, settings: dict[str, object]) -> None:
        self._settings = settings
        for fn in self._settings_handlers:
            self._handlers.run_now("on_settings_changed", fn, self._settings)

    def _report_handler_error(self, origin: str, exc: Exception) -> None:
        short_traceback = "".join(traceback.format_exception(exc, limit=-3)).strip()
        with contextlib.suppress(OSError):
            self.log(
                "error",
                f"{origin} handler raised {type(exc).__name__}: {exc}",
                traceback=short_traceback,
            )
