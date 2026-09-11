"""Shared injected-IO helpers for Plugin behavior tests."""

from __future__ import annotations

import io
import json
import queue
import threading
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

from smabar_sdk import Plugin


def request(
    request_id: int, method: str, params: dict[str, object] | None = None
) -> dict[str, object]:
    return {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {}}


def notification(method: str, params: dict[str, object]) -> dict[str, object]:
    return {"jsonrpc": "2.0", "method": method, "params": params}


def init_params(**overrides: object) -> dict[str, object]:
    params: dict[str, object] = {
        "protocolVersion": 1,
        "pluginId": "demo",
        "dataDir": "/tmp/demo-data",
        "pluginDir": "/tmp/demo-code",
        "settings": {"name": "gerome"},
        "language": "en",
        "locale": {"hello": "Hallo"},
        "providers": ["cpu", "memory", "disk", "network", "battery", "media"],
    }
    params.update(overrides)
    return params


def ndjson(messages: list[dict[str, object]]) -> str:
    return "".join(json.dumps(message) + "\n" for message in messages)


def initialized(writer: io.StringIO) -> Plugin:
    """A plugin that has completed the handshake, as the core performs it.

    Anything reading locales, settings or rendering before that point is the
    mistake the SDK now warns about, so tests of the wire format start here.
    """
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "shutdown")]))
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    writer.seek(0)
    writer.truncate(0)
    return plugin


def sent(writer: io.StringIO) -> list[dict[str, Any]]:
    messages: list[dict[str, Any]] = []
    for line in writer.getvalue().splitlines():
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:  # torn read while the plugin thread is writing
            continue
    return messages


def run_plugin_thread(plugin: Plugin) -> threading.Thread:
    thread = threading.Thread(target=plugin.run, daemon=True)
    thread.start()
    return thread


def wait_until(condition: Callable[[], bool], timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():
        if time.monotonic() > deadline:
            pytest.fail("condition not met within timeout")
        time.sleep(0.005)


class QueueReader:
    """Blocking line source so tests can feed messages to a running plugin."""

    def __init__(self) -> None:
        self._lines: queue.Queue[str | None] = queue.Queue()

    def feed(self, message: dict[str, object]) -> None:
        self._lines.put(json.dumps(message) + "\n")

    def close(self) -> None:
        self._lines.put(None)

    def readline(self) -> str:
        item = self._lines.get()
        return "" if item is None else item


def write_locale(plugin_dir: Path, language: str, strings: dict[str, str] | str) -> None:
    locales_dir = plugin_dir / "locales"
    locales_dir.mkdir(exist_ok=True)
    content = strings if isinstance(strings, str) else json.dumps(strings)
    (locales_dir / f"{language}.json").write_text(content, encoding="utf-8")


def run_to_shutdown(params: dict[str, object]) -> tuple[Plugin, io.StringIO]:
    reader = io.StringIO(ndjson([request(1, "initialize", params), request(2, "shutdown")]))
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    return plugin, writer
