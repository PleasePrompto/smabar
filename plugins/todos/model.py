"""Local todos and reminder state. Every acknowledged write is a SQLite commit."""

from __future__ import annotations

import json
import sqlite3
import uuid
from datetime import datetime
from pathlib import Path
from typing import Callable


class TodoError(ValueError):
    """An actionable validation or revision conflict."""


def text(args: dict, key: str, maximum: int, *, required: bool = False) -> str:
    value = args.get(key, "")
    if (
        not isinstance(value, str)
        or len(value) > maximum
        or (required and not value.strip())
    ):
        raise TodoError(
            f"{key} must be {'a non-empty' if required else 'a'} string up to {maximum} characters"
        )
    return value.strip()


def integer(args: dict, key: str, minimum: int, maximum: int) -> int:
    value = args.get(key)
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not minimum <= value <= maximum
    ):
        raise TodoError(f"{key} must be an integer from {minimum} to {maximum}")
    return value


class Todos:
    def __init__(self, path: Path, now_ms: Callable[[], int]) -> None:
        self.now_ms = now_ms
        self.db = sqlite3.connect(path, autocommit=False)
        self.db.row_factory = sqlite3.Row
        version = self.db.execute("PRAGMA user_version").fetchone()[0]
        if version not in (0, 1):
            self.db.close()
            raise TodoError(
                "Unsupported todo database version; upgrade the plugin before opening it"
            )
        if version == 0:
            with self.db:
                self.db.execute("""CREATE TABLE todos (
                    id TEXT PRIMARY KEY, title TEXT NOT NULL, note TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('open','done')),
                    revision INTEGER NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
                    reminder_at_ms INTEGER, reminder_id TEXT,
                    delivery_state TEXT NOT NULL CHECK(delivery_state IN ('none','pending','shown','dismissed','suppressed','dropped')),
                    shown_at_ms INTEGER
                )""")
                self.db.execute(
                    "CREATE INDEX todos_due ON todos(status, reminder_at_ms)"
                )
                self.db.execute("""CREATE TABLE requests (
                    request_id TEXT PRIMARY KEY, command TEXT NOT NULL,
                    arguments TEXT NOT NULL, result TEXT NOT NULL
                )""")
                self.db.execute("PRAGMA user_version=1")

    def close(self) -> None:
        self.db.close()

    def get(self, todo_id: str) -> dict:
        row = self.db.execute("SELECT * FROM todos WHERE id=?", (todo_id,)).fetchone()
        if row is None:
            raise TodoError("Todo does not exist; refresh the list")
        return self._output(row)

    @staticmethod
    def _output(row: sqlite3.Row) -> dict:
        return {
            "id": row["id"],
            "title": row["title"],
            "note": row["note"],
            "status": row["status"],
            "revision": row["revision"],
            "createdMs": row["created_ms"],
            "updatedMs": row["updated_ms"],
            "reminderAtMs": row["reminder_at_ms"],
            "reminderId": row["reminder_id"],
            "deliveryState": row["delivery_state"],
            "shownAtMs": row["shown_at_ms"],
        }

    def list(self, status: str = "open", *, limit: int = 100, offset: int = 0) -> dict:
        if status not in ("open", "done", "all"):
            raise TodoError("status must be open, done or all")
        rows = self.db.execute(
            "SELECT * FROM todos WHERE (?='all' OR status=?) ORDER BY created_ms DESC, id LIMIT ? OFFSET ?",
            (status, status, limit + 1, offset),
        ).fetchall()
        page = []
        size = 0
        for row in rows[:limit]:
            item = self._output(row)
            size += len(json.dumps(item).encode("utf-8"))
            if page and size > 400 * 1024:
                break  # Leave room for the RPC envelope below its 1 MiB limit.
            page.append(item)
        return {
            "todos": page,
            "nextOffset": offset + len(page) if len(rows) > len(page) else None,
        }

    def due(self) -> list[dict]:
        return [
            self._output(row)
            for row in self.db.execute(
                "SELECT * FROM todos WHERE status='open' AND reminder_at_ms<=? AND delivery_state NOT IN ('none','dismissed') ORDER BY reminder_at_ms,id",
                (self.now_ms(),),
            )
        ]

    def delivery(self, reminder_ids: list[str], state: str) -> None:
        if state not in ("shown", "dismissed", "suppressed", "dropped"):
            return
        with self.db:
            self.db.executemany(
                "UPDATE todos SET delivery_state=?, shown_at_ms=CASE WHEN ?='shown' THEN coalesce(shown_at_ms,?) ELSE shown_at_ms END WHERE reminder_id=? AND status='open'",
                [
                    (state, state, self.now_ms(), reminder_id)
                    for reminder_id in reminder_ids
                ],
            )

    def _deadline(self, args: dict, now: int) -> int | None:
        if "remindInMinutes" in args and "remindAt" in args:
            raise TodoError("use either remindInMinutes or remindAt")
        if "remindInMinutes" in args:
            return now + integer(args, "remindInMinutes", 1, 525600) * 60000
        value = args.get("remindAt")
        if value is None:
            return None
        if not isinstance(value, str):
            raise TodoError("remindAt must be an ISO timestamp with timezone or null")
        try:
            parsed = datetime.fromisoformat(value)
            if parsed.tzinfo is None or parsed.utcoffset() is None:
                raise ValueError("missing timezone")
            timestamp = round(parsed.timestamp() * 1000)
        except (ValueError, OverflowError, OSError) as exc:
            raise TodoError(
                "remindAt needs a valid ISO timestamp with timezone offset"
            ) from exc
        if not 0 <= timestamp <= 253402214400000:
            raise TodoError("remindAt is outside the supported calendar range")
        return timestamp

    def call(self, command: str, args: dict) -> dict:
        allowed = {
            "todos.list": {"status", "limit", "offset"},
            "todos.get": {"id"},
            "todos.create": {
                "requestId",
                "title",
                "note",
                "remindAt",
                "remindInMinutes",
            },
            "todos.update": {
                "requestId",
                "id",
                "revision",
                "title",
                "note",
                "remindAt",
                "remindInMinutes",
            },
            "todos.snooze": {"requestId", "id", "revision", "minutes"},
            "todos.complete": {"requestId", "id", "revision"},
            "todos.reopen": {"requestId", "id", "revision"},
            "todos.delete": {"requestId", "id", "revision"},
        }
        if command not in allowed or set(args) - allowed[command]:
            raise TodoError("unknown command or arguments; consult plugin_commands")
        if command == "todos.list":
            values = {"limit": 100, "offset": 0, **args}
            return self.list(
                text(args, "status", 10) or "open",
                limit=integer(values, "limit", 1, 100),
                offset=integer(values, "offset", 0, 2147483647),
            )
        if command == "todos.get":
            return {"todo": self.get(text(args, "id", 128, required=True))}
        request_id = text(args, "requestId", 128, required=True)
        serialized = json.dumps(
            args, sort_keys=True, separators=(",", ":"), allow_nan=False
        )
        with self.db:
            previous = self.db.execute(
                "SELECT command,arguments,result FROM requests WHERE request_id=?",
                (request_id,),
            ).fetchone()
            if previous:
                if (
                    previous["command"] != command
                    or previous["arguments"] != serialized
                ):
                    raise TodoError(
                        "requestId was already used with different arguments"
                    )
                return json.loads(previous["result"])
            result = self._write(command, args)
            self.db.execute(
                "INSERT INTO requests VALUES (?,?,?,?)",
                (request_id, command, serialized, json.dumps(result)),
            )
        return result

    def _write(self, command: str, args: dict) -> dict:
        now = self.now_ms()
        if command == "todos.create":
            todo_id = str(uuid.uuid4())
            title = text(args, "title", 240, required=True)
            note = text(args, "note", 10000)
            deadline = self._deadline(args, now)
            self.db.execute(
                "INSERT INTO todos VALUES (?,?,?,'open',1,?,?,?,?,?,NULL)",
                (
                    todo_id,
                    title,
                    note,
                    now,
                    now,
                    deadline,
                    str(uuid.uuid4()) if deadline is not None else None,
                    "pending" if deadline is not None else "none",
                ),
            )
            return {"todo": self.get(todo_id)}
        todo_id = text(args, "id", 128, required=True)
        current = self.get(todo_id)
        if integer(args, "revision", 1, 2147483647) != current["revision"]:
            raise TodoError("Todo changed; fetch its current revision before editing")
        if command == "todos.delete":
            self.db.execute("DELETE FROM todos WHERE id=?", (todo_id,))
            return {"deletedId": todo_id}
        title, note = current["title"], current["note"]
        status, deadline = current["status"], current["reminderAtMs"]
        reminder_id, delivery = current["reminderId"], current["deliveryState"]
        new_reminder = False
        if command == "todos.update":
            if "title" in args:
                title = text(args, "title", 240, required=True)
            if "note" in args:
                note = text(args, "note", 10000)
            if "remindAt" in args or "remindInMinutes" in args:
                deadline = self._deadline(args, now)
                if status != "open" and deadline is not None:
                    raise TodoError("Reopen the todo before scheduling a reminder")
                new_reminder = deadline != current["reminderAtMs"]
        elif command == "todos.snooze":
            if status != "open":
                raise TodoError("Only open todos can be snoozed")
            deadline = now + integer(args, "minutes", 1, 525600) * 60000
            new_reminder = True
        elif command in ("todos.complete", "todos.reopen"):
            status = "done" if command == "todos.complete" else "open"
            deadline = None
            new_reminder = True
        if new_reminder:
            reminder_id = str(uuid.uuid4()) if deadline is not None else None
            delivery = "pending" if deadline is not None else "none"
        self.db.execute(
            """UPDATE todos SET title=?,note=?,status=?,revision=revision+1,updated_ms=?,
            reminder_at_ms=?,reminder_id=?,delivery_state=?,shown_at_ms=? WHERE id=?""",
            (
                title,
                note,
                status,
                now,
                deadline,
                reminder_id,
                delivery,
                None if new_reminder else current["shownAtMs"],
                todo_id,
            ),
        )
        return {"todo": self.get(todo_id)}
