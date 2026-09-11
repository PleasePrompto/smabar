"""Plugin-owned commands with real results, registered before run()."""

from __future__ import annotations

import copy
import re
from collections.abc import Callable

from smabar_sdk._handlers import SerializedHandlers
from smabar_sdk.protocol import INTERNAL_ERROR, METHOD_NOT_FOUND, RpcConnection, RpcError

type CommandHandler = Callable[[dict[str, object]], dict[str, object]]


class Commands:
    def __init__(
        self,
        conn: RpcConnection,
        handlers: SerializedHandlers,
        report_error: Callable[[str, Exception], None],
    ) -> None:
        self._conn = conn
        self._handlers = handlers
        self._report_error = report_error
        self._registered: dict[str, tuple[dict[str, object], CommandHandler]] = {}
        self._sealed = False

    def register(
        self,
        name: str,
        *,
        description: str,
        input_schema: dict[str, object],
        output_schema: dict[str, object],
    ) -> Callable[[CommandHandler], CommandHandler]:
        """Schemas describe the contract; handlers must validate runtime arguments."""
        if self._sealed:
            raise RuntimeError("register commands before initialize")
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", name):
            raise ValueError("command name must use 1-128 letters, digits, '.', '_' or '-'")
        if not description or len(description.encode()) > 4096:
            raise ValueError("command description must use 1-4096 UTF-8 bytes")

        def register(fn: CommandHandler) -> CommandHandler:
            if self._sealed:
                raise RuntimeError("register commands before initialize")
            if name in self._registered or len(self._registered) >= 64:
                raise ValueError("commands need unique names; at most 64 are supported")
            self._registered[name] = (
                {
                    "name": name,
                    "description": description,
                    "inputSchema": copy.deepcopy(input_schema),
                    "outputSchema": copy.deepcopy(output_schema),
                },
                fn,
            )
            return fn

        return register

    def describe(self) -> list[dict[str, object]]:
        self._sealed = True
        return [copy.deepcopy(info) for info, _ in self._registered.values()]

    def dispatch(self, request_id: object, params: dict[str, object]) -> None:
        name = params.get("command")
        entry = self._registered.get(name) if isinstance(name, str) else None
        if entry is None:
            self._conn.send_error(request_id, METHOD_NOT_FOUND, "unknown plugin command")
            return
        arguments = params.get("arguments")
        if not isinstance(arguments, dict):
            self._conn.send_error(request_id, -32602, "command arguments must be an object")
            return

        def execute() -> None:
            try:
                result = entry[1](arguments)
                if not isinstance(result, dict):
                    raise TypeError("command handler must return a JSON object")
                self._conn.send_result(request_id, result)
            except RpcError as exc:
                self._conn.send_error(request_id, exc.code, exc.message)
            except ValueError as exc:
                self._conn.send_error(request_id, -32602, str(exc))
            except Exception as exc:  # log the cause, return a safe protocol error
                self._report_error(f"command({name!r})", exc)
                self._conn.send_error(
                    request_id, INTERNAL_ERROR, "command failed; inspect plugin_logs"
                )

        if not self._handlers.try_submit(f"command({name!r})", execute):
            self._conn.send_error(request_id, -32001, "plugin handler queue is full or stopped")
