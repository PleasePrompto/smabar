# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Agent-maintained local todos with durable reminders and actionable toasts."""

import json
import sqlite3
import time
import uuid
from datetime import datetime

from smabar_sdk import Plugin, RpcError
from model import Todos, TodoError
from schemas import COMMANDS
from views import action, flyout, input_time, reminder, tile
from html import escape

app = Plugin()
TILE = "todos"
database: Todos | None = None
view = "open"
offset = 0
editing: str | None = None
edit_revision = 0
edit_reminder = ""
form_epoch = 0
summary_expanded = False
error = False
published: dict[str, str] = {}
shown_popups: set[str] = set()
popup_reminders: dict[str, list[str]] = {}
retry_after: dict[str, int] = {}
display_error: str | None = None


def db() -> Todos:
    if database is None:
        raise TodoError("Todo storage is unavailable; inspect plugin_logs")
    return database


def snooze_minutes() -> int:
    value = app.settings.get("snoozeMinutes", 20)
    return (
        value
        if isinstance(value, int)
        and not isinstance(value, bool)
        and 1 <= value <= 525600
        else 20
    )


def redraw() -> None:
    data = db()
    count = data.db.execute(
        "SELECT count(*) FROM todos WHERE status='open'"
    ).fetchone()[0]
    next_due = data.db.execute(
        "SELECT min(reminder_at_ms) FROM todos WHERE status='open' AND delivery_state!='dismissed'"
    ).fetchone()[0]
    app.render(TILE, "tile", tile(count, next_due, app.t))
    current = None
    if editing is not None:
        try:
            current = data.get(editing)
        except TodoError:
            pass  # a concurrent agent delete removes the edit form
    app.render(
        TILE,
        "flyout",
        flyout(
            data.list(view, limit=20, offset=offset),
            app.t,
            view,
            current,
            offset,
            error,
            form_epoch,
        ),
    )


def reconcile() -> None:
    due = db().due()
    wanted: dict[str, tuple[str, list[str], bool]] = {}
    if len(due) > 5:
        ids = [item["reminderId"] for item in due]
        html = (
            f'<p class="sb-title">{escape(app.t("overdue").replace("{count}", str(len(due))))}</p>'
            + action("show_overdue", app.t("showTodos"))
        )
        if summary_expanded:
            page = {
                "todos": due[offset : offset + 20],
                "nextOffset": offset + 20 if len(due) > offset + 20 else None,
            }
            html = flyout(page, app.t, "open", None, offset, error, form_epoch)
        wanted["overdue"] = (html, ids, any(item["shownAtMs"] is None for item in due))
    else:
        for todo in due:
            wanted[todo["reminderId"]] = (
                reminder(todo, app.t, snooze_minutes(), error),
                [todo["reminderId"]],
                todo["shownAtMs"] is None,
            )
    for popup_id in list(published):
        if popup_id not in wanted:
            app.popups.dismiss(TILE, popup_id)
            published.pop(popup_id, None)
            shown_popups.discard(popup_id)
            popup_reminders.pop(popup_id, None)
    for popup_id, (html, ids, sound) in wanted.items():
        popup_reminders[popup_id] = ids
        if published.get(popup_id) == html or time.time() < retry_after.get(
            popup_id, 0
        ):
            continue
        source = (
            {"root": "plugin", "path": "sounds/reminder.wav"}
            if sound and app.settings.get("sound", True)
            else None
        )
        result = app.popups.show(TILE, popup_id, html, sound=source)
        popup_reminders[popup_id] = ids
        if result.get("state") == "suppressed":
            db().delivery(ids, "suppressed")
            retry_after[popup_id] = int(time.time()) + 30
        else:
            published[popup_id] = html
            retry_after.pop(popup_id, None)
            if popup_id in shown_popups:
                db().delivery(ids, "shown")


def update_surfaces() -> None:
    global display_error
    try:
        redraw()
        reconcile()
        display_error = None
    except (RpcError, OSError, RuntimeError, sqlite3.Error) as exc:
        # Data was already committed. A presentation failure must not pretend it rolled back.
        message = str(exc)
        if message != display_error:
            app.log(
                "error",
                "Todo surfaces could not refresh; retry the display",
                error=message,
            )
            display_error = message


def register_commands() -> None:
    for name, (description, properties, required, output) in COMMANDS.items():

        def handle(
            arguments: dict[str, object], command: str = name
        ) -> dict[str, object]:
            result = db().call(command, arguments)
            if command not in ("todos.list", "todos.get"):
                update_surfaces()
            return result

        app.command(
            name,
            description=description,
            input_schema={
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": False,
            },
            output_schema=output,
        )(handle)


@app.on_ready
def ready() -> None:
    global database
    database = Todos(
        app.data_dir / "todos.sqlite3", lambda: time.time_ns() // 1_000_000
    )
    update_surfaces()


@app.every(1)
def tick() -> None:
    update_surfaces()


@app.popups.on_event
def popup_event(event: dict[str, object]) -> None:
    popup_id = event.get("popupId")
    state = event.get("state")
    if not isinstance(popup_id, str) or not isinstance(state, str):
        return
    ids = popup_reminders.get(popup_id, [])
    if state == "dismissed" and event.get("reason") == "plugin":
        return
    db().delivery(ids, state)
    if state == "shown":
        shown_popups.add(popup_id)
    if state in ("dismissed", "expired", "suppressed", "dropped"):
        published.pop(popup_id, None)
        shown_popups.discard(popup_id)
        if state in ("suppressed", "dropped"):
            retry_after[popup_id] = int(time.time()) + 30
    redraw()


@app.on_action(TILE)
def on_action(name: str, value: object) -> None:
    global \
        view, \
        offset, \
        editing, \
        edit_revision, \
        edit_reminder, \
        form_epoch, \
        summary_expanded, \
        error
    error = False
    try:
        if name == "view":
            if value not in ("open", "done"):
                raise TodoError("invalid view")
            view, offset = value, 0
        elif name == "page":
            offset = max(0, int(str(value)))
        elif name == "edit":
            current = db().get(str(value))
            editing, edit_revision = current["id"], current["revision"]
            edit_reminder = input_time(current["reminderAtMs"])
        elif name == "cancel":
            editing = None
        elif name == "show_overdue":
            summary_expanded = True
            published.pop("overdue", None)
        elif name == "save":
            if not isinstance(value, dict):
                raise TodoError("invalid form")
            key = editing or f"new-{form_epoch}"
            args = {
                "requestId": str(uuid.uuid4()),
                "title": value.get(f"title-{key}", ""),
                "note": value.get(f"note-{key}", ""),
            }
            raw = value.get(f"reminder-{key}", "")
            if not editing or raw != edit_reminder:
                args["remindAt"] = (
                    datetime.fromisoformat(raw).astimezone().isoformat()
                    if raw
                    else None
                )
            command = "todos.create"
            if editing:
                # Revision is captured when editing starts; never overwrite a concurrent edit.
                args.update({"id": editing, "revision": edit_revision})
                command = "todos.update"
            db().call(command, args)
            editing = None
            form_epoch += 1
        elif name in ("complete", "reopen", "delete", "snooze", "notify"):
            args = json.loads(value) if isinstance(value, str) else value
            if not isinstance(args, dict):
                raise TodoError("invalid todo action")
            args["requestId"] = str(uuid.uuid4())
            if name == "notify":
                current = db().get(args["id"])
                if current["revision"] != args.get("revision"):
                    raise TodoError("Todo changed")
                published.pop(current["reminderId"], None)
                retry_after.pop(current["reminderId"], None)
            else:
                if name == "snooze":
                    args["minutes"] = snooze_minutes()
                db().call("todos." + name, args)
    except (ValueError, TypeError, OSError, RpcError, sqlite3.Error) as exc:
        error = True
        app.log(
            "warn",
            "Todo action failed; refresh the todo and retry",
            action=name,
            error=str(exc),
        )
    update_surfaces()


register_commands()
if __name__ == "__main__":
    app.run()
