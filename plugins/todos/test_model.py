"""Persistence and reminder regressions for the bundled todo domain service."""

import json
from pathlib import Path

import pytest

import model as module


def test_reminder_snooze_recovery_and_idempotent_writes(tmp_path: Path) -> None:
    now = [1_000_000]
    path = tmp_path / "todos.sqlite3"
    todos = module.Todos(path, lambda: now[0])
    create = {"requestId": "create", "title": "Review", "remindInMinutes": 20}
    saved = todos.call("todos.create", create)
    task = saved["todo"]
    assert task["reminderAtMs"] == now[0] + 20 * 60_000
    assert todos.call("todos.create", create) == saved
    assert len(todos.list()["todos"]) == 1
    assert todos.due() == []
    todos.close()

    now[0] += 21 * 60_000
    todos = module.Todos(path, lambda: now[0])
    assert todos.due()[0]["id"] == task["id"]
    snooze = {"requestId": "snooze", "id": task["id"], "revision": 1, "minutes": 20}
    snoozed = todos.call("todos.snooze", snooze)["todo"]
    assert snoozed["reminderId"] != task["reminderId"]
    assert snoozed["reminderAtMs"] == now[0] + 20 * 60_000
    assert todos.due() == []
    todos.delivery([task["reminderId"]], "dismissed")
    assert todos.get(task["id"])["deliveryState"] == "pending"
    completed = todos.call("todos.complete", {"requestId": "done", "id": task["id"], "revision": 2})
    assert completed["todo"]["status"] == "done"
    reopened = todos.call("todos.reopen", {"requestId": "open", "id": task["id"], "revision": 3})
    assert reopened["todo"]["reminderAtMs"] is None
    todos.close()


def test_conflicts_and_invalid_input_leave_data_intact(tmp_path: Path) -> None:
    todos = module.Todos(tmp_path / "todos.sqlite3", lambda: 1_000_000)
    args = {"requestId": "a", "title": "A"}
    task = todos.call("todos.create", args)["todo"]
    for command, change in [
        ("todos.create", {**args, "title": "B"}),
        ("todos.create", {"requestId": "b", "title": "B", "remindAt": "2026-09-05T12:00:00"}),
        ("todos.create", {"requestId": "c", "title": "B", "remindInMinutes": True}),
        ("todos.update", {"requestId": "d", "id": task["id"], "revision": 9, "title": "B"}),
        ("todos.update", {"requestId": "e", "id": task["id"], "revision": 1, "title": ""}),
    ]:
        with pytest.raises(ValueError):
            todos.call(command, change)
    assert todos.get(task["id"]) == task
    assert len(todos.list()["todos"]) == 1
    todos.close()


def test_dismissed_reminders_stay_dismissed_after_restart(tmp_path: Path) -> None:
    path = tmp_path / "todos.sqlite3"
    todos = module.Todos(path, lambda: 1_000_000)
    task = todos.call(
        "todos.create", {"requestId": "a", "title": "A", "remindAt": "1970-01-01T00:00:01+00:00"}
    )["todo"]
    todos.delivery([task["reminderId"]], "suppressed")
    assert len(todos.due()) == 1
    todos.delivery([task["reminderId"]], "dismissed")
    todos.close()
    todos = module.Todos(path, lambda: 9_000_000)
    assert todos.due() == []
    assert todos.get(task["id"])["status"] == "open"
    todos.close()


def test_edit_preserves_delivery_and_completed_todos_can_be_edited(tmp_path: Path) -> None:
    todos = module.Todos(tmp_path / "todos.sqlite3", lambda: 1_000_000)
    deadline = "1970-01-01T00:00:01+00:00"
    task = todos.call("todos.create", {"requestId": "a", "title": "A", "remindAt": deadline})[
        "todo"
    ]
    todos.delivery([task["reminderId"]], "shown")
    edit = {
        "requestId": "b",
        "id": task["id"],
        "revision": 1,
        "title": "Edited",
        "remindAt": deadline,
    }
    updated = todos.call("todos.update", edit)["todo"]
    assert updated["reminderId"] == task["reminderId"]
    assert updated["deliveryState"] == "shown"
    assert updated["shownAtMs"] == 1_000_000
    todos.call("todos.complete", {"requestId": "c", "id": task["id"], "revision": 2})
    updated = todos.call(
        "todos.update", {**edit, "requestId": "d", "revision": 3, "remindAt": None}
    )["todo"]
    assert updated["status"] == "done"
    assert updated["title"] == "Edited"
    todos.close()


def test_large_notes_are_paginated_below_the_transport_limit(tmp_path: Path) -> None:
    todos = module.Todos(tmp_path / "todos.sqlite3", lambda: 1_000_000)
    for index in range(20):
        todos.call(
            "todos.create", {"requestId": str(index), "title": str(index), "note": "😀" * 10000}
        )
    offset = 0
    received = set()
    while True:
        page = todos.call("todos.list", {"offset": offset})
        assert len(json.dumps(page).encode()) < 512 * 1024
        received.update(item["id"] for item in page["todos"])
        if page["nextOffset"] is None:
            break
        assert page["nextOffset"] > offset
        offset = page["nextOffset"]
    assert len(received) == 20
    todos.close()
