"""Provider availability and action wire tests for the public SDK."""

from __future__ import annotations

import io
import json
import queue
import threading
import time
from collections.abc import Callable
from typing import Any

import pytest

from smabar_sdk import Plugin


def request(request_id: int, method: str, params: dict[str, object]) -> dict[str, object]:
    return {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}


def notification(method: str, params: dict[str, object]) -> dict[str, object]:
    return {"jsonrpc": "2.0", "method": method, "params": params}


def init_params(**overrides: object) -> dict[str, object]:
    params: dict[str, object] = {
        "protocolVersion": 1,
        "pluginId": "demo",
        "dataDir": "/tmp/demo-data",
        "pluginDir": "/tmp/demo-code",
        "settings": {},
        "language": "en",
        "locale": {},
        "providers": ["cpu", "memory", "disk", "network", "battery", "media"],
    }
    params.update(overrides)
    return params


class QueueReader:
    def __init__(self) -> None:
        self._lines: queue.Queue[str | None] = queue.Queue()

    def feed(self, message: dict[str, object]) -> None:
        self._lines.put(json.dumps(message) + "\n")

    def readline(self) -> str:
        line = self._lines.get()
        return "" if line is None else line


def sent(writer: io.StringIO) -> list[dict[str, Any]]:
    messages: list[dict[str, Any]] = []
    for line in writer.getvalue().splitlines():
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return messages


def wait_until(condition: Callable[[], bool], timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():
        if time.monotonic() > deadline:
            pytest.fail("condition not met within timeout")
        time.sleep(0.005)


def start(plugin: Plugin) -> threading.Thread:
    thread = threading.Thread(target=plugin.run, daemon=True)
    thread.start()
    return thread


def stop(reader: QueueReader, thread: threading.Thread) -> None:
    reader.feed(request(99, "shutdown", {}))
    thread.join(timeout=5)
    assert not thread.is_alive()


def test_unavailable_provider_is_skipped_and_logged_once() -> None:
    reader = QueueReader()
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)

    @plugin.on_provider("media")
    def on_media(_data: dict[str, object]) -> None:
        pytest.fail("an unavailable provider must not dispatch data")

    thread = start(plugin)
    reader.feed(request(1, "initialize", init_params(providers=["cpu"])))
    wait_until(
        lambda: any(
            message.get("method") == "log"
            and "not available on this platform" in str(message.get("params"))
            for message in sent(writer)
        )
    )
    messages = sent(writer)
    assert not any(message.get("method") == "provider.subscribe" for message in messages)
    logs = [
        message
        for message in messages
        if message.get("method") == "log"
        and "not available on this platform" in str(message.get("params"))
    ]
    assert len(logs) == 1
    assert "@app.on_ready" in logs[0]["params"]["message"]
    stop(reader, thread)


def test_missing_provider_advertisement_is_rejected() -> None:
    params = init_params()
    params.pop("providers")
    reader = io.StringIO(json.dumps(request(1, "initialize", params)) + "\n")
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    response = sent(writer)[0]
    assert response["error"]["code"] == -32603
    assert "providers" in response["error"]["message"]


def test_provider_action_uses_the_typed_rpc_wire() -> None:
    reader = QueueReader()
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    action_finished = threading.Event()

    @plugin.on_action("player", "next")
    def next_track(_action: str, _value: object) -> None:
        plugin.provider_action(
            "media",
            "next",
            sessionId="org.mpris.MediaPlayer2.spotify",
        )
        action_finished.set()

    thread = start(plugin)
    reader.feed(request(1, "initialize", init_params()))
    wait_until(lambda: any(message.get("id") == 1 for message in sent(writer)))
    reader.feed(notification("event", {"tileId": "player", "action": "next"}))
    wait_until(lambda: any(m.get("method") == "provider.action" for m in sent(writer)))
    action_request = next(m for m in sent(writer) if m.get("method") == "provider.action")
    assert action_request["params"] == {
        "kind": "media",
        "action": "next",
        "sessionId": "org.mpris.MediaPlayer2.spotify",
    }
    reader.feed({"jsonrpc": "2.0", "id": action_request["id"], "result": {}})
    wait_until(action_finished.is_set)
    stop(reader, thread)
