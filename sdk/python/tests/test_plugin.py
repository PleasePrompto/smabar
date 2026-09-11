"""Behavioral tests for the smabar-sdk Plugin class over injected IO streams."""

from __future__ import annotations

import io
import time
from pathlib import Path
from typing import cast

import pytest
from _plugin_test_support import (
    QueueReader,
    init_params,
    initialized,
    ndjson,
    notification,
    request,
    run_plugin_thread,
    run_to_shutdown,
    sent,
    wait_until,
    write_locale,
)

from smabar_sdk import Plugin, RpcError


def test_initialize_handshake() -> None:
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "shutdown")]))
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    assert sent(writer)[0] == {"jsonrpc": "2.0", "id": 1, "result": {}}
    assert plugin.plugin_id == "demo"
    assert plugin.data_dir == Path("/tmp/demo-data")
    assert plugin.settings == {"name": "gerome"}
    assert plugin.locale == {"hello": "Hallo"}


def test_public_settings_and_locale_are_detached_snapshots() -> None:
    nested = {"channels": [{"id": "one"}]}
    plugin, _writer = run_to_shutdown(init_params(settings=nested))

    settings = plugin.settings
    locale = plugin.locale
    cast(list[dict[str, str]], settings["channels"])[0]["id"] = "changed"
    locale["hello"] = "Changed"

    assert plugin.settings == nested
    assert plugin.locale == {"hello": "Hallo"}
    assert plugin.t("hello") == "Hallo"


def test_data_dir_and_plugin_dir_are_separate() -> None:
    plugin, _writer = run_to_shutdown(
        init_params(dataDir="/tmp/demo-data", pluginDir="/tmp/demo-code")
    )
    assert plugin.data_dir == Path("/tmp/demo-data")
    assert plugin.plugin_dir == Path("/tmp/demo-code")


def test_initialize_rejects_a_missing_plugin_dir() -> None:
    params = init_params()
    params.pop("pluginDir")
    reader = io.StringIO(ndjson([request(1, "initialize", params)]))
    writer = io.StringIO()
    Plugin(reader=reader, writer=writer).run()
    response = sent(writer)[0]
    assert response["id"] == 1
    assert response["error"]["code"] == -32603
    assert "pluginDir" in response["error"]["message"]


def test_locales_are_read_from_the_code_folder_not_the_data_dir(tmp_path: Path) -> None:
    code_dir = tmp_path / "code"
    code_dir.mkdir()
    write_locale(code_dir, "en", {"hello": "From code"})
    data_dir = tmp_path / "data"
    data_dir.mkdir()
    write_locale(data_dir, "en", {"hello": "From data"})
    plugin, _writer = run_to_shutdown(init_params(dataDir=str(data_dir), pluginDir=str(code_dir)))
    assert plugin.t("hello") == "From code"


def test_properties_raise_before_initialize() -> None:
    plugin = Plugin(reader=io.StringIO(), writer=io.StringIO())
    with pytest.raises(RuntimeError, match="initialize"):
        _ = plugin.plugin_id
    with pytest.raises(RuntimeError, match="initialize"):
        _ = plugin.data_dir
    with pytest.raises(RuntimeError, match="initialize"):
        _ = plugin.plugin_dir


def test_render_and_log_emit_single_line_notifications() -> None:
    writer = io.StringIO()
    plugin = initialized(writer)
    plugin.render("hello", "tile", "<div>hi</div>")
    plugin.log("info", "started", attempt=1)
    plugin.log("debug", "no fields")
    assert writer.getvalue().count("\n") == 3
    messages = sent(writer)
    assert messages[0] == {
        "jsonrpc": "2.0",
        "method": "ui.render",
        "params": {"tileId": "hello", "target": "tile", "html": "<div>hi</div>"},
    }
    assert messages[1] == {
        "jsonrpc": "2.0",
        "method": "log",
        "params": {"level": "info", "message": "started", "fields": {"attempt": 1}},
    }
    assert messages[2] == {
        "jsonrpc": "2.0",
        "method": "log",
        "params": {"level": "debug", "message": "no fields"},
    }


def test_popup_render_emits_optional_ttl() -> None:
    writer = io.StringIO()
    plugin = initialized(writer)
    plugin.render("hello", "popup", "<div>Heads up</div>", ttl_ms=9000)
    assert sent(writer) == [
        {
            "jsonrpc": "2.0",
            "method": "ui.render",
            "params": {
                "tileId": "hello",
                "target": "popup",
                "html": "<div>Heads up</div>",
                "ttlMs": 9000,
            },
        }
    ]


def test_using_plugin_data_before_initialize_warns_once() -> None:
    writer = io.StringIO()
    plugin = Plugin(reader=io.StringIO(), writer=writer)
    # No locales yet, so t() can only echo the key back — the symptom of a
    # plugin that renders at import time.
    assert plugin.t("clock.title") == "clock.title"
    plugin.render("hello", "tile", "<div>hi</div>")
    _ = plugin.settings

    warnings = [m for m in sent(writer) if m["method"] == "log"]
    assert len(warnings) == 1, "the guard must not spam the log"
    assert warnings[0]["params"]["level"] == "warn"
    assert "initialize" in warnings[0]["params"]["message"]
    assert "on_ready" in warnings[0]["params"]["message"]


def test_on_ready_runs_once_after_initialize_with_locales_and_settings() -> None:
    seen: list[tuple[str, object]] = []
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "shutdown")]))
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)

    @plugin.on_ready
    def first_render() -> None:
        # The whole point: both are populated by the time this runs.
        seen.append((plugin.t("hello"), plugin.settings.get("name")))
        plugin.render("hello", "tile", "<div>ready</div>")

    plugin.run()

    assert seen == [("Hallo", "gerome")]
    renders = [m for m in sent(writer) if m.get("method") == "ui.render"]
    assert len(renders) == 1
    # And no early-use warning was emitted.
    assert [m for m in sent(writer) if m.get("method") == "log"] == []


def test_on_action_dispatch_specific_catch_all_and_unknown_tile() -> None:
    calls: list[tuple[str, str, object]] = []
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params()),
                notification("event", {"tileId": "hello", "action": "click", "value": 5}),
                notification("event", {"tileId": "hello", "action": "hover"}),
                notification("event", {"tileId": "ghost", "action": "click"}),
                request(2, "shutdown"),
            ]
        )
    )
    plugin = Plugin(reader=reader, writer=io.StringIO())

    @plugin.on_action("hello", action="click")
    def on_click(action: str, value: object) -> None:
        calls.append(("specific", action, value))

    @plugin.on_action("hello")
    def on_any(action: str, value: object) -> None:
        calls.append(("catch-all", action, value))

    plugin.run()
    assert calls == [
        ("specific", "click", 5),
        ("catch-all", "click", 5),
        ("catch-all", "hover", None),
    ]


def test_handler_exception_is_logged_and_loop_continues() -> None:
    calls: list[str] = []
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params()),
                notification("event", {"tileId": "hello", "action": "boom"}),
                notification("event", {"tileId": "hello", "action": "ok"}),
                request(2, "shutdown"),
            ]
        )
    )
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)

    @plugin.on_action("hello")
    def handler(action: str, value: object) -> None:
        if action == "boom":
            raise ValueError("kaboom")
        calls.append(action)

    plugin.run()
    assert calls == ["ok"]
    error_logs = [
        message
        for message in sent(writer)
        if message.get("method") == "log" and message["params"]["level"] == "error"
    ]
    assert len(error_logs) == 1
    assert "ValueError" in error_logs[0]["params"]["message"]
    assert "kaboom" in error_logs[0]["params"]["message"]
    assert "kaboom" in error_logs[0]["params"]["fields"]["traceback"]


def test_shutdown_sends_result_and_terminates_run() -> None:
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params()),
                request(2, "shutdown"),
                request(3, "ping"),
            ]
        )
    )
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    thread = run_plugin_thread(plugin)
    thread.join(timeout=5)
    assert not thread.is_alive()
    responded_ids = [message.get("id") for message in sent(writer)]
    assert 2 in responded_ids
    assert 3 not in responded_ids  # nothing after shutdown is processed


def test_ping_unknown_method_and_broken_lines() -> None:
    body = (
        ndjson([request(1, "initialize", init_params())])
        + "this is not json\n"
        + ndjson([request(2, "ping"), request(3, "frobnicate"), request(4, "shutdown")])
    )
    writer = io.StringIO()
    plugin = Plugin(reader=io.StringIO(body), writer=writer)
    plugin.run()
    responses = {message["id"]: message for message in sent(writer) if "id" in message}
    assert responses[2] == {"jsonrpc": "2.0", "id": 2, "result": {}}
    assert responses[3]["error"]["code"] == -32601
    assert responses[4] == {"jsonrpc": "2.0", "id": 4, "result": {}}


def test_get_settings_sends_request_and_returns_result() -> None:
    reader = io.StringIO(ndjson([{"jsonrpc": "2.0", "id": 1, "result": {"settings": {"a": 1}}}]))
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    assert plugin.get_settings() == {"a": 1}
    assert sent(writer)[0] == {"jsonrpc": "2.0", "id": 1, "method": "settings.get", "params": {}}


def test_request_error_response_raises_rpc_error() -> None:
    reader = io.StringIO(
        ndjson([{"jsonrpc": "2.0", "id": 1, "error": {"code": -32001, "message": "nope"}}])
    )
    plugin = Plugin(reader=reader, writer=io.StringIO())
    with pytest.raises(RpcError, match="nope"):
        plugin.set_settings({"x": 1})


def test_locale_lookup_falls_back_to_key() -> None:
    reader = io.StringIO(ndjson([request(1, "initialize", init_params()), request(2, "shutdown")]))
    plugin = Plugin(reader=reader, writer=io.StringIO())
    plugin.run()
    assert plugin.t("hello") == "Hallo"
    assert plugin.t("missing.key") == "missing.key"


def test_settings_changed_updates_state_and_notifies_handler() -> None:
    seen: list[dict[str, object]] = []
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params()),
                notification("settings.changed", {"settings": {"name": "neo"}}),
                request(2, "shutdown"),
            ]
        )
    )
    plugin = Plugin(reader=reader, writer=io.StringIO())

    @plugin.on_settings_changed
    def changed(settings: dict[str, object]) -> None:
        seen.append(settings)

    plugin.run()
    assert plugin.settings == {"name": "neo"}
    assert seen == [{"name": "neo"}]


def test_every_timer_starts_after_initialize_and_stops_on_shutdown() -> None:
    reader = QueueReader()
    plugin = Plugin(reader=reader, writer=io.StringIO())
    ticks: list[float] = []

    @plugin.every(60.0)
    def tick() -> None:
        ticks.append(time.monotonic())

    thread = run_plugin_thread(plugin)
    assert ticks == []  # timers only start after initialize
    reader.feed(request(1, "initialize", init_params()))
    wait_until(lambda: len(ticks) == 1)  # first run is immediate
    reader.feed(request(2, "shutdown"))
    thread.join(timeout=5)
    assert not thread.is_alive()
    assert len(ticks) == 1  # 60s interval: no second run before shutdown


def test_provider_subscribe_and_data_dispatch() -> None:
    reader = QueueReader()
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    received: list[dict[str, object]] = []

    @plugin.on_provider("cpu", interval_ms=500)
    def on_cpu(data: dict[str, object]) -> None:
        received.append(data)

    thread = run_plugin_thread(plugin)
    reader.feed(request(1, "initialize", init_params()))
    wait_until(lambda: any(m.get("method") == "provider.subscribe" for m in sent(writer)))
    subscribe = next(m for m in sent(writer) if m.get("method") == "provider.subscribe")
    assert subscribe["params"] == {"kind": "cpu", "intervalMs": 500}
    reader.feed({"jsonrpc": "2.0", "id": subscribe["id"], "result": {}})
    reader.feed(
        notification("provider.data", {"kind": "cpu", "data": {"usage": 42.5}, "tsMs": 1000})
    )
    wait_until(lambda: received == [{"usage": 42.5}])
    reader.feed(request(2, "shutdown"))
    thread.join(timeout=5)
    assert not thread.is_alive()


@pytest.mark.parametrize("interval_ms", [True, -1, 249, 3_600_001, 500.5, "500"])
def test_provider_subscription_rejects_unsupported_intervals(interval_ms: object) -> None:
    plugin = Plugin(reader=io.StringIO(), writer=io.StringIO())

    with pytest.raises(ValueError, match="interval_ms"):
        plugin.on_provider("cpu", cast(int, interval_ms))


def test_eof_terminates_run() -> None:
    plugin = Plugin(reader=io.StringIO(""), writer=io.StringIO())
    plugin.run()  # returns immediately at EOF instead of blocking
