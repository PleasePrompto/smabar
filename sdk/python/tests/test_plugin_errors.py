"""Error-path tests: broken locale files, raising handlers, malformed ids.

Split from test_plugin.py to respect the 500-line limit; the fixtures here
mirror the standalone-test convention the other plugin test files use.
"""

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


def ndjson(messages: list[dict[str, object]]) -> str:
    return "".join(json.dumps(message) + "\n" for message in messages)


def init_params(**overrides: object) -> dict[str, object]:
    params: dict[str, object] = {
        "protocolVersion": 1,
        "pluginId": "demo",
        "dataDir": "/tmp/demo-data",
        "pluginDir": "/tmp/demo-code",
        "settings": {},
        "language": "en",
        "locale": {},
        "providers": [],
    }
    params.update(overrides)
    return params


def sent(writer: io.StringIO) -> list[dict[str, Any]]:
    return [json.loads(line) for line in writer.getvalue().splitlines()]


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


def test_non_utf8_locale_file_warns_and_plugin_keeps_running(tmp_path: Path) -> None:
    """A locale file saved as Latin-1/CP1252 is skipped like broken JSON —
    it must not crash the plugin during initialize (and crash-loop it)."""
    locales_dir = tmp_path / "locales"
    locales_dir.mkdir()
    (locales_dir / "en.json").write_bytes(b'{"hello": "Sch\xf6ne"}')  # Latin-1 bytes
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params(pluginDir=str(tmp_path))),
                request(2, "ping"),
                request(3, "shutdown"),
            ]
        )
    )
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    warnings = [
        m for m in sent(writer) if m.get("method") == "log" and m["params"]["level"] == "warn"
    ]
    assert len(warnings) == 1
    assert "cannot read locale file" in warnings[0]["params"]["message"]
    responded_ids = {m["id"] for m in sent(writer) if "id" in m}
    assert responded_ids == {1, 2, 3}  # every request answered; the loop lived


class _InitCrasher(Plugin):
    def _apply_initialize(self, params: dict[str, object]) -> None:
        raise RuntimeError("boom")


def test_initialize_failure_answers_internal_error_and_stops() -> None:
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "ping")]))
    writer = io.StringIO()
    plugin = _InitCrasher(reader=reader, writer=writer)
    plugin.run()  # returns after the failed initialize — no crash, no zombie loop
    messages = sent(writer)
    assert messages[0]["id"] == 1
    assert messages[0]["error"]["code"] == -32603  # INTERNAL_ERROR, not EOF
    assert "boom" in messages[0]["error"]["message"]
    assert len(messages) == 1  # the loop stopped; the stale ping was never served


def test_initialize_setup_failure_does_not_send_a_second_response(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "ping")]))
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)

    @plugin.every(1)
    def tick() -> None:
        pass

    original_start = threading.Thread.start

    def fail_start(thread: threading.Thread) -> None:
        if thread.name.startswith("smabar-every-"):
            raise RuntimeError("timer setup failed")
        original_start(thread)

    monkeypatch.setattr(threading.Thread, "start", fail_start)

    plugin.run()

    messages = sent(writer)
    responses = [message for message in messages if message.get("id") == 1]
    assert responses == [{"jsonrpc": "2.0", "id": 1, "result": {}}]
    errors = [message for message in messages if message.get("method") == "log"]
    assert len(errors) == 1
    assert errors[0]["params"]["level"] == "error"
    assert "timer setup failed" in errors[0]["params"]["fields"]["error"]


class _SetupAndLogCrasher(Plugin):
    def _start_timers(self) -> None:
        raise RuntimeError("timer setup failed")

    def log(self, level: str, message: str, **fields: object) -> None:
        raise ValueError("logger failed")


def test_initialize_setup_and_error_log_failure_still_send_one_response() -> None:
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "ping")]))
    writer = io.StringIO()
    plugin = _SetupAndLogCrasher(reader=reader, writer=writer)

    plugin.run()

    assert sent(writer) == [{"jsonrpc": "2.0", "id": 1, "result": {}}]


class _PingCrasher(Plugin):
    def _dispatch_request(self, request_id: object, method: str, params: dict[str, object]) -> bool:
        if method == "ping":
            raise RuntimeError("boom")
        return super()._dispatch_request(request_id, method, params)


def test_a_raising_request_answers_internal_error_and_keeps_serving() -> None:
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params()),
                request(2, "ping"),
                request(3, "shutdown"),
            ]
        )
    )
    writer = io.StringIO()
    plugin = _PingCrasher(reader=reader, writer=writer)
    plugin.run()
    messages = sent(writer)
    assert messages[0] == {"jsonrpc": "2.0", "id": 1, "result": {}}
    assert messages[1]["id"] == 2
    assert messages[1]["error"]["code"] == -32603
    assert messages[2] == {"jsonrpc": "2.0", "id": 3, "result": {}}  # loop survived


def test_bool_response_ids_do_not_resolve_pending_requests() -> None:
    """bool is an int subclass: a response with "id": true must not resolve
    pending request 1 (hash(True) == hash(1))."""
    reader = QueueReader()
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    thread = run_plugin_thread(plugin)
    result_box: list[dict[str, object]] = []

    def call() -> None:
        result_box.append(plugin.get_settings())

    worker = threading.Thread(target=call)
    worker.start()
    wait_until(lambda: any(m.get("method") == "settings.get" for m in sent(writer)))
    reader.feed({"jsonrpc": "2.0", "id": True, "result": {"settings": {"wrong": 1}}})
    reader.feed({"jsonrpc": "2.0", "id": 1, "result": {"settings": {"right": 1}}})
    worker.join(timeout=5)
    reader.close()
    thread.join(timeout=5)
    assert result_box == [{"right": 1}]
