"""Concurrency guarantees of the SDK's stdout JSON-RPC transport."""

from __future__ import annotations

import io
import json
import queue
import threading
import time

import pytest

from smabar_sdk import Plugin
from smabar_sdk.protocol import RpcConnection, RpcTimeoutError


class FragmentingWriter(io.StringIO):
    """A stream that exposes overlapping write calls as torn NDJSON."""

    def __init__(self) -> None:
        super().__init__()
        self._state_lock = threading.Lock()
        self._probe = False
        self._active = 0
        self.max_active = 0
        self.thread_names: set[str] = set()
        self.reader_write_started = threading.Event()

    def start_probe(self) -> None:
        self.seek(0)
        self.truncate(0)
        self._probe = True
        self.max_active = 0
        self.thread_names.clear()
        self.reader_write_started.clear()

    def write(self, data: str, /) -> int:
        if not self._probe:
            return super().write(data)
        thread_name = threading.current_thread().name
        with self._state_lock:
            self.thread_names.add(thread_name)
            self._active += 1
            self.max_active = max(self.max_active, self._active)
        if thread_name == "rpc-reader":
            self.reader_write_started.set()
        try:
            middle = len(data) // 2
            written = super().write(data[:middle])
            time.sleep(0.01)  # release the GIL while the other workers are ready
            return written + super().write(data[middle:])
        finally:
            with self._state_lock:
                self._active -= 1


class QueueReader:
    """Blocking stdin stand-in that lets the test feed live core requests."""

    def __init__(self) -> None:
        self._lines: queue.Queue[str] = queue.Queue()

    def push(self, message: dict[str, object]) -> None:
        self._lines.put(json.dumps(message) + "\n")

    def readline(self) -> str:
        return self._lines.get()


def test_render_and_log_from_worker_threads_keep_ndjson_lines_atomic() -> None:
    writer = FragmentingWriter()
    reader = QueueReader()
    plugin = Plugin(reader=reader, writer=writer)
    ready = threading.Event()

    @plugin.on_ready
    def mark_ready() -> None:
        ready.set()

    runner = threading.Thread(target=plugin.run, name="rpc-reader")
    runner.start()
    reader.push(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "pluginId": "test",
                "dataDir": "/missing",
                "pluginDir": "/missing",
                "providers": [],
            },
        }
    )
    assert ready.wait(timeout=5), "plugin did not initialize"
    writer.start_probe()

    worker_count = 8
    ping_count = 8

    def publish(index: int) -> None:
        payload = f"worker-{index}-" + "x" * 200
        if index % 2 == 0:
            plugin.render("status", "tile", payload)
        else:
            plugin.log("info", payload)

    workers = [
        threading.Thread(target=publish, args=(index,), name=f"publisher-{index}")
        for index in range(worker_count)
    ]
    for worker in workers[:2]:
        worker.start()
    for worker in workers[:2]:
        worker.join(timeout=5)
        assert not worker.is_alive(), "worker deadlocked while publishing"

    reader.push(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "ping",
            "params": {},
        }
    )
    assert writer.reader_write_started.wait(timeout=5), "reader did not answer live ping"

    for worker in workers[2:]:
        worker.start()
    for request_id in range(11, 10 + ping_count):
        reader.push(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "ping",
                "params": {},
            }
        )
    for worker in workers[2:]:
        worker.join(timeout=5)
        assert not worker.is_alive(), "worker deadlocked while publishing"
    reader.push(
        {
            "jsonrpc": "2.0",
            "id": 99,
            "method": "shutdown",
            "params": {},
        }
    )
    runner.join(timeout=5)
    assert not runner.is_alive(), "RPC reader did not stop"

    lines = writer.getvalue().splitlines()
    messages = [json.loads(line) for line in lines]
    assert len(messages) == worker_count + ping_count + 1
    assert writer.max_active == 1, "render/log writes overlapped"
    methods = [message.get("method") for message in messages]
    assert methods.count("ui.render") == worker_count // 2
    assert methods.count("log") == worker_count // 2
    first_ping = next(index for index, message in enumerate(messages) if message.get("id") == 10)
    worker_messages = [
        index for index, method in enumerate(methods) if method in {"ui.render", "log"}
    ]
    assert worker_messages[0] < first_ping < worker_messages[-1]
    response_ids = {message.get("id") for message in messages if "result" in message}
    assert response_ids == {*range(10, 10 + ping_count), 99}
    assert "rpc-reader" in writer.thread_names
    assert any(name.startswith("publisher-") for name in writer.thread_names)


def test_timer_rpc_does_not_block_a_settings_notification() -> None:
    reader = QueueReader()
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    seen: list[tuple[str, object]] = []

    @plugin.every(60)
    def tick() -> None:
        seen.append(("timer", plugin.get_settings()))

    @plugin.on_settings_changed
    def settings_changed(settings: dict[str, object]) -> None:
        seen.append(("settings", settings))

    runner = threading.Thread(target=plugin.run, daemon=True)
    runner.start()
    reader.push(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "pluginId": "test",
                "dataDir": "/missing",
                "pluginDir": "/missing",
                "settings": {},
                "providers": [],
            },
        }
    )
    deadline = time.monotonic() + 5
    while '"method":"settings.get"' not in writer.getvalue():
        assert time.monotonic() < deadline, "timer did not request settings"
        time.sleep(0.005)

    reader.push(
        {"jsonrpc": "2.0", "method": "settings.changed", "params": {"settings": {"new": 1}}}
    )
    reader.push({"jsonrpc": "2.0", "id": 1, "result": {"settings": {"rpc": 1}}})
    deadline = time.monotonic() + 5
    while len(seen) < 2:
        assert time.monotonic() < deadline, "timer/settings handlers deadlocked"
        time.sleep(0.005)

    reader.push({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": {}})
    runner.join(timeout=5)
    assert not runner.is_alive()
    assert seen == [("timer", {"rpc": 1}), ("settings", {"new": 1})]


def test_request_timeout_does_not_depend_on_readline_returning() -> None:
    connection = RpcConnection(QueueReader(), io.StringIO(), timeout=0.02)

    started = time.monotonic()
    with pytest.raises(RpcTimeoutError, match=r"settings\.get"):
        connection.send_request("settings.get", {})

    assert time.monotonic() - started < 1


def test_failed_request_write_removes_its_pending_entry() -> None:
    class FailingWriter:
        def write(self, _data: str, /) -> int:
            raise OSError("pipe closed")

        def flush(self) -> None:
            pass

    connection = RpcConnection(QueueReader(), FailingWriter())

    with pytest.raises(OSError, match="pipe closed"):
        connection.send_request("settings.get", {})

    assert connection._pending == {}
