"""JSON-RPC 2.0 over NDJSON: framing, atomic writes, request/response correlation.

stdout belongs to the protocol: every outgoing message is exactly one JSON line,
built as a single string, written under a lock, and flushed immediately.
"""

from __future__ import annotations

import json
import queue
import threading
from typing import Protocol

JSONRPC_VERSION = "2.0"
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INTERNAL_ERROR = -32603
TRANSPORT_ERROR = -32000
INCOMING_QUEUE_CAPACITY = 64


class Reader(Protocol):
    """The part of a text stream the SDK reads from (sys.stdin in production)."""

    def readline(self) -> str: ...


class Writer(Protocol):
    """The part of a text stream the SDK writes to (sys.stdout in production)."""

    def write(self, data: str, /) -> int: ...

    def flush(self) -> None: ...


class RpcError(Exception):
    """An error response from the core, or a transport-level failure."""

    def __init__(self, code: int, message: str) -> None:
        super().__init__(f"[{code}] {message}")
        self.code = code
        self.message = message


class RpcTimeoutError(RpcError):
    """No response arrived within the request timeout."""

    def __init__(self, method: str, timeout: float) -> None:
        super().__init__(TRANSPORT_ERROR, f"no response to {method!r} within {timeout:g}s")


class _Pending:
    __slots__ = ("error", "event", "result")

    def __init__(self) -> None:
        self.event = threading.Event()
        self.result: object = None
        self.error: RpcError | None = None


class RpcConnection:
    """Writes NDJSON messages atomically and correlates outgoing requests with responses."""

    def __init__(self, reader: Reader, writer: Writer, timeout: float = 10.0) -> None:
        self._reader = reader
        self._writer = writer
        self._timeout = timeout
        self._write_lock = threading.Lock()
        self._state_lock = threading.Lock()
        self._next_id = 0
        self._pending: dict[int, _Pending] = {}
        self._incoming: queue.Queue[dict[str, object] | None] = queue.Queue(INCOMING_QUEUE_CAPACITY)
        self._reader_started = False
        self._closed = False

    def read_message(self) -> dict[str, object] | None:
        """Return the next request/notification, or None at EOF."""
        self._start_reader()
        return self._incoming.get()

    def send_notification(self, method: str, params: dict[str, object]) -> None:
        self._write({"jsonrpc": JSONRPC_VERSION, "method": method, "params": params})

    def send_result(self, request_id: object, result: dict[str, object]) -> None:
        self._write({"jsonrpc": JSONRPC_VERSION, "id": request_id, "result": result})

    def send_error(self, request_id: object, code: int, message: str) -> None:
        error: dict[str, object] = {"code": code, "message": message}
        self._write({"jsonrpc": JSONRPC_VERSION, "id": request_id, "error": error})

    def send_request(self, method: str, params: dict[str, object]) -> object:
        """Send a request and block until its response arrives or the timeout hits.

        Raises RpcError for an error response and RpcTimeoutError on timeout.
        """
        pending = _Pending()
        with self._state_lock:
            if self._closed:
                raise RpcError(TRANSPORT_ERROR, "connection is closed")
            self._next_id += 1
            request_id = self._next_id
            self._pending[request_id] = pending
        try:
            self._write(
                {
                    "jsonrpc": JSONRPC_VERSION,
                    "id": request_id,
                    "method": method,
                    "params": params,
                }
            )
            self._start_reader()
            if not pending.event.wait(self._timeout):
                raise RpcTimeoutError(method, self._timeout)
        finally:
            with self._state_lock:
                self._pending.pop(request_id, None)
        if pending.error is not None:
            raise pending.error
        return pending.result

    def resolve(self, message: dict[str, object]) -> bool:
        """Route a response to its waiting request. Returns False if it is not a response."""
        if "method" in message or ("result" not in message and "error" not in message):
            return False
        request_id = message.get("id")
        pending: _Pending | None = None
        # bool is an int subclass: `{"id": true}` must not resolve request 1.
        if isinstance(request_id, int) and not isinstance(request_id, bool):
            with self._state_lock:
                pending = self._pending.pop(request_id, None)
        if pending is None:
            return True  # a response nobody waits for any more (e.g. after a timeout)
        error = message.get("error")
        if isinstance(error, dict):
            code = error.get("code")
            pending.error = RpcError(
                code if isinstance(code, int) else INTERNAL_ERROR,
                str(error.get("message", "unknown error")),
            )
        else:
            pending.result = message.get("result")
        pending.event.set()
        return True

    def fail_pending(self, message: str) -> None:
        """Unblock every waiting request with a transport error (used on EOF/shutdown)."""
        with self._state_lock:
            self._closed = True
            pending_list = list(self._pending.values())
            self._pending.clear()
        for pending in pending_list:
            pending.error = RpcError(TRANSPORT_ERROR, message)
            pending.event.set()

    def _start_reader(self) -> None:
        with self._state_lock:
            if self._reader_started:
                return
            thread = threading.Thread(
                target=self._reader_loop,
                name="smabar-rpc-reader",
                daemon=True,
            )
            thread.start()
            self._reader_started = True

    def _reader_loop(self) -> None:
        while True:
            line = self._reader.readline()
            if line == "":
                self.fail_pending("connection closed")
                self._incoming.put(None)
                return
            message = _parse_line(line)
            if message is None:
                continue
            if not self.resolve(message):
                self._incoming.put(message)

    def _write(self, message: dict[str, object]) -> None:
        line = json.dumps(message, separators=(",", ":"), allow_nan=False) + "\n"
        if len(line.encode("utf-8")) > 1024 * 1024:
            raise ValueError("RPC message exceeds the host's 1 MiB limit; paginate large results")
        with self._write_lock:
            self._writer.write(line)
            self._writer.flush()


def _parse_line(line: str) -> dict[str, object] | None:
    stripped = line.strip()
    if not stripped:
        return None
    try:
        parsed: object = json.loads(stripped)
    except json.JSONDecodeError:
        return None
    if isinstance(parsed, dict) and parsed.get("jsonrpc") == JSONRPC_VERSION:
        return parsed
    return None
