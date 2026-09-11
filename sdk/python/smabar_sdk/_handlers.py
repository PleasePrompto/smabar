"""One serialized worker for every user callback registered on a plugin."""

from __future__ import annotations

import queue
import threading
from collections.abc import Callable

type HandlerCall = tuple[str, Callable[..., None], tuple[object, ...], threading.Event | None]
type ErrorHandler = Callable[[str, Exception], None]
HANDLER_QUEUE_CAPACITY = 64


class SerializedHandlers:
    """Runs callbacks in registration order without blocking the RPC reader."""

    def __init__(self, stopped: threading.Event, on_error: ErrorHandler) -> None:
        self._stopped = stopped
        self._on_error = on_error
        self._queue: queue.Queue[HandlerCall | None] = queue.Queue(HANDLER_QUEUE_CAPACITY)
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._loop,
            name="smabar-handler",
            daemon=True,
        )
        self._thread.start()

    def submit(
        self,
        origin: str,
        fn: Callable[..., None],
        *args: object,
        wait: bool = False,
    ) -> None:
        done = threading.Event() if wait else None
        call = (origin, fn, args, done)
        while not self._stopped.is_set():
            try:
                self._queue.put(call, timeout=0.1)
                break
            except queue.Full:
                continue
        else:
            return
        if done is not None:
            while not done.wait(0.1):
                if self._stopped.is_set():
                    return

    def run_now(self, origin: str, fn: Callable[..., None], *args: object) -> None:
        try:
            fn(*args)
        except Exception as exc:  # a broken callback must never kill the plugin
            self._on_error(origin, exc)

    def try_submit(self, origin: str, fn: Callable[..., None], *args: object) -> bool:
        """Queue a request without blocking the protocol loop on a busy handler."""
        if self._stopped.is_set():
            return False
        try:
            self._queue.put_nowait((origin, fn, args, None))
            return True
        except queue.Full:
            return False

    def stop(self) -> None:
        self._stopped.set()
        try:
            self._queue.put(None, timeout=1.0)
        except queue.Full:
            return
        if self._thread is not None:
            self._thread.join(timeout=1.0)

    def _loop(self) -> None:
        while (call := self._queue.get()) is not None:
            origin, fn, args, done = call
            try:
                self.run_now(origin, fn, *args)
            finally:
                if done is not None:
                    done.set()
