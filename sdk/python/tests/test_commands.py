"""Commands acknowledge execution without blocking nested host replies or ping."""

import io
import threading

from _plugin_test_support import (
    QueueReader,
    init_params,
    request,
    run_plugin_thread,
    sent,
    wait_until,
)

from smabar_sdk import Plugin
from smabar_sdk.protocol import RpcError


def test_command_discovers_returns_committed_result_and_allows_host_calls() -> None:
    reader, writer = QueueReader(), io.StringIO()
    app = Plugin(reader=reader, writer=writer)
    entered = threading.Event()

    @app.command(
        "save",
        description="Save",
        input_schema={"type": "object"},
        output_schema={"type": "object"},
    )
    def save(arguments: dict[str, object]) -> dict[str, object]:
        entered.set()
        app.set_settings(arguments)
        return {"saved": arguments}

    runner = run_plugin_thread(app)
    reader.feed(request(1, "initialize", init_params()))
    wait_until(lambda: any(m.get("id") == 1 for m in sent(writer)))
    assert sent(writer)[0]["result"]["commands"][0]["name"] == "save"
    reader.feed(request(2, "command.call", {"command": "save", "arguments": {"task": 1}}))
    assert entered.wait(2)
    wait_until(lambda: any(m.get("method") == "settings.set" for m in sent(writer)))
    assert not any(m.get("id") == 2 and "result" in m for m in sent(writer))
    reader.feed(request(3, "ping"))
    wait_until(lambda: any(m.get("id") == 3 for m in sent(writer)))
    host_call = next(m for m in sent(writer) if m.get("method") == "settings.set")
    reader.feed({"jsonrpc": "2.0", "id": host_call["id"], "result": {}})
    wait_until(lambda: any(m.get("id") == 2 and "result" in m for m in sent(writer)))
    assert next(m for m in sent(writer) if m.get("id") == 2)["result"] == {"saved": {"task": 1}}
    reader.feed(request(4, "shutdown"))
    runner.join(3)
    reader.close()
    assert not runner.is_alive()


def test_failed_command_returns_error_without_stopping_plugin() -> None:
    reader, writer = QueueReader(), io.StringIO()
    app = Plugin(reader=reader, writer=writer)

    @app.command("fail", description="Fail", input_schema={}, output_schema={})
    def fail(_arguments: dict[str, object]) -> dict[str, object]:
        raise RpcError(-32002, "revision conflict; refresh the todo")

    runner = run_plugin_thread(app)
    reader.feed(request(1, "initialize", init_params()))
    reader.feed(request(2, "command.call", {"command": "fail", "arguments": {}}))
    wait_until(lambda: any(m.get("id") == 2 for m in sent(writer)))
    assert next(m for m in sent(writer) if m.get("id") == 2)["error"]["code"] == -32002
    reader.feed(request(3, "command.call", {"command": "missing", "arguments": {}}))
    wait_until(lambda: any(m.get("id") == 3 for m in sent(writer)))
    assert next(m for m in sent(writer) if m.get("id") == 3)["error"]["code"] == -32601
    reader.feed(request(4, "shutdown"))
    runner.join(3)
    reader.close()
    assert not runner.is_alive()
