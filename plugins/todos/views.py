"""Theme-aware UI-kit composition; all interactions use the shell contract."""

from collections.abc import Callable
from datetime import datetime
from html import escape
import json

Translator = Callable[[str], str]


def icon(name: str) -> str:
    return f'<span data-lucide="{name}" aria-hidden="true"></span>'


def action(
    name: str,
    label: str,
    value: object = "",
    *,
    primary: bool = False,
    symbol: str = "",
    icon_only: bool = False,
) -> str:
    encoded = (
        value if isinstance(value, str) else json.dumps(value, separators=(",", ":"))
    )
    classes = "sb-btn sb-btn-primary" if primary else "sb-btn sb-btn-ghost"
    if icon_only:
        classes += " sb-btn-icon"
    accessible = (
        f' aria-label="{escape(label, quote=True)}" title="{escape(label, quote=True)}"'
        if icon_only
        else ""
    )
    content = (icon(symbol) if symbol else "") + ("" if icon_only else escape(label))
    return f'<button type="button" class="{classes}" data-action="{escape(name)}" data-value="{escape(encoded, quote=True)}"{accessible}>{content}</button>'


def when(timestamp: int | None) -> str:
    return (
        datetime.fromtimestamp(timestamp / 1000)
        .astimezone()
        .strftime("%d.%m.%Y · %H:%M")
        if timestamp is not None
        else ""
    )


def input_time(timestamp: int | None) -> str:
    return (
        datetime.fromtimestamp(timestamp / 1000).astimezone().strftime("%Y-%m-%dT%H:%M")
        if timestamp is not None
        else ""
    )


def note_preview(note: str) -> str:
    return escape(note[:1000]).replace("\n", "<br>") + ("…" if len(note) > 1000 else "")


def failure(t: Translator) -> str:
    return (
        f'<div class="sb-alert sb-alert--danger" role="alert">'
        f'<span class="sb-alert__icon">{icon("circle-alert")}</span>'
        f'<div><p class="sb-alert__title">{escape(t("errorTitle"))}</p>'
        f'<p class="sb-alert__text">{escape(t("error"))}</p></div></div>'
    )


def tile(count: int, next_due: int | None, t: Translator) -> str:
    caption = (
        when(next_due)
        if next_due is not None
        else t("allClear" if count == 0 else "noReminder")
    )
    return (
        f'<div class="sb-tile">{icon("circle-check" if count == 0 else "list")}'
        f'<span class="sb-tile-stack"><span><span class="sb-mono">{count}</span> {escape(t("open"))}</span>'
        f'<span class="sb-muted">{escape(caption)}</span></span></div>'
    )


def reminder(todo: dict, t: Translator, snooze: int, error: bool = False) -> str:
    value = {"id": todo["id"], "revision": todo["revision"]}
    return (
        f'<div class="sb-inline sb-meta">{icon("bell")}<span>{escape(t("reminder"))}</span></div>'
        f'<h2 class="sb-title sb-wrap">{escape(todo["title"])}</h2>'
        f'<p class="sb-meta">{escape(when(todo["reminderAtMs"]))}</p>'
        + (
            f'<p class="sb-dim sb-wrap">{note_preview(todo["note"])}</p>'
            if todo["note"]
            else ""
        )
        + (failure(t) if error else "")
        + '<div class="sb-cluster">'
        + action("complete", t("done"), value, primary=True, symbol="check")
        + action(
            "snooze",
            t("snooze").replace("{minutes}", str(snooze)),
            value,
            symbol="timer",
        )
        + "</div>"
    )


def task_card(todo: dict, t: Translator) -> str:
    done = todo["status"] == "done"
    value = {"id": todo["id"], "revision": todo["revision"]}
    rows = [
        '<article class="sb-card sb-wrap">',
        f'<h3 class="sb-text-m {"sb-muted" if done else ""}">{escape(todo["title"])}</h3>',
    ]
    if todo["note"]:
        rows.append(f'<p class="sb-text-s sb-dim">{note_preview(todo["note"])}</p>')
    if todo["reminderAtMs"] is not None:
        rows.append(
            f'<p class="sb-inline sb-meta">{icon("bell")}<span>{escape(when(todo["reminderAtMs"]))}</span></p>'
        )
    if todo["deliveryState"] in ("dropped", "suppressed"):
        rows.append(
            f'<p class="sb-text-s sb-warn">{escape(t("undelivered"))}</p>'
            + action("notify", t("retryReminder"), value, symbol="refresh-cw")
        )
    rows.append(
        '<div class="sb-inline">'
        + action(
            "reopen" if done else "complete",
            t("reopen" if done else "done"),
            value,
            symbol="rotate-cw" if done else "circle-check",
        )
        + '<div class="sb-inline sb-push">'
        + action("edit", t("edit"), todo["id"], symbol="pencil", icon_only=True)
        + action("delete", t("delete"), value, symbol="trash-2", icon_only=True)
        + "</div></div></article>"
    )
    return "".join(rows)


def editor(t: Translator, edit: dict | None, form_epoch: int, expanded: bool) -> str:
    key = edit["id"] if edit else f"new-{form_epoch}"
    title_id, note_id, reminder_id = (
        f"{field}-{key}" for field in ("title", "note", "reminder")
    )
    title = escape(edit["title"], quote=True) if edit else ""
    note = escape(edit["note"]) if edit else ""
    deadline = input_time(edit["reminderAtMs"]) if edit else ""
    heading = escape(t("editTask" if edit else "add"))
    return (
        f'<details class="sb-accordion-item" id="editor-{key}" {"open" if expanded else ""}>'
        f'<summary>{heading}</summary><div class="sb-accordion-item__body">'
        '<form class="sb-stack">'
        f'<div class="sb-field-stack"><label class="sb-field__label" for="{title_id}">{escape(t("taskTitle"))}</label>'
        f'<input class="sb-input" id="{title_id}" data-field="{title_id}" type="text" required maxlength="240" value="{title}" placeholder="{escape(t("titlePlaceholder"), quote=True)}" aria-describedby="{title_id}-error">'
        f'<p class="sb-field__error" id="{title_id}-error">{escape(t("titleRequired"))}</p></div>'
        f'<div class="sb-field-stack"><label class="sb-field__label" for="{note_id}">{escape(t("note"))} <span class="sb-meta">· {escape(t("optional"))}</span></label>'
        f'<textarea class="sb-textarea" id="{note_id}" data-field="{note_id}" maxlength="10000" rows="3" placeholder="{escape(t("notePlaceholder"), quote=True)}">{note}</textarea></div>'
        f'<div class="sb-field-stack"><label class="sb-field__label" for="{reminder_id}">{escape(t("reminder"))} <span class="sb-meta">· {escape(t("optional"))}</span></label>'
        f'<input class="sb-input" id="{reminder_id}" data-field="{reminder_id}" data-sb-temporal type="datetime-local" value="{deadline}" aria-describedby="{reminder_id}-hint">'
        f'<p class="sb-field__hint" id="{reminder_id}-hint">{escape(t("reminderHint"))}</p></div>'
        '<div class="sb-cluster">'
        f'<button type="submit" class="sb-btn sb-btn-primary" data-action="save">{icon("check" if edit else "plus")}{escape(t("save" if edit else "create"))}</button>'
        + (action("cancel", t("cancel")) if edit else "")
        + "</div></form></div></details>"
    )


def flyout(
    page: dict,
    t: Translator,
    status: str,
    edit: dict | None,
    offset: int,
    error: bool,
    form_epoch: int = 0,
) -> str:
    rows = [
        f'<div class="sb-header"><span class="sb-icon-badge">{icon("list")}</span>'
        f'<div><h2 class="sb-title">{escape(t("title"))}</h2><p class="sb-meta">{escape(t("subtitle"))}</p></div></div>',
        f'<div class="sb-tabs" role="group" aria-label="{escape(t("filter"), quote=True)}">',
    ]
    for value, label, symbol in [
        ("open", "open", "circle"),
        ("done", "completed", "circle-check"),
    ]:
        selected = status == value
        rows.append(
            f'<button type="button" class="sb-tab {"sb-active" if selected else ""}" aria-pressed="{str(selected).lower()}" data-action="view" data-value="{value}">{icon(symbol)}{escape(t(label))}</button>'
        )
    rows.append("</div>")
    if error:
        rows.append(failure(t))
    if not page["todos"] and edit is None:
        done = status == "done"
        rows.append(
            f'<div class="sb-empty"><div class="sb-empty__icon sb-accent">{icon("archive" if done else "circle-check")}</div>'
            f'<h3 class="sb-empty__title">{escape(t("emptyDoneTitle" if done else "emptyOpenTitle"))}</h3>'
            f'<p class="sb-empty__text">{escape(t("emptyDoneHint" if done else "emptyOpenHint"))}</p></div>'
        )
    if status == "open" or edit is not None:
        rows.append(
            editor(t, edit, form_epoch, bool(edit) or error or not page["todos"])
        )
    if page["todos"]:
        rows.append('<div class="sb-stack">')
        rows.extend(task_card(todo, t) for todo in page["todos"])
        rows.append("</div>")
    if offset > 0 or page["nextOffset"] is not None:
        rows.append('<div class="sb-cluster">')
        if offset > 0:
            rows.append(
                action(
                    "page",
                    t("previous"),
                    str(max(0, offset - 20)),
                    symbol="chevron-left",
                )
            )
        if page["nextOffset"] is not None:
            rows.append(
                action(
                    "page", t("next"), str(page["nextOffset"]), symbol="chevron-right"
                )
            )
        rows.append("</div>")
    return "".join(rows)
