"""Clock markup: plain semantic HTML on the smabar UI kit, no styling of its own.

Everything that ticks is a `data-clock-text` / `data-clock` node the shell keeps
alive; this module only decides WHAT is shown, never when to redraw.
"""

import json
from collections.abc import Callable
from datetime import date, datetime, timedelta
from html import escape

Translator = Callable[[str], str]


def icon(name: str, title: str = "") -> str:
    hint = f' title="{escape(title, quote=True)}"' if title else ""
    return f'<span data-lucide="{name}" aria-hidden="true"{hint}></span>'


def action(
    name: str,
    label: str,
    value: object = "",
    *,
    primary: bool = False,
    symbol: str = "",
    icon_only: bool = False,
    pressed: bool | None = None,
    ghost: bool = True,
) -> str:
    encoded = value if isinstance(value, str) else json.dumps(value, separators=(",", ":"))
    classes = "sb-btn sb-btn-primary" if primary else ("sb-btn sb-btn-ghost" if ghost else "sb-btn")
    if icon_only:
        classes += " sb-btn-icon"
    accessible = (
        f' aria-label="{escape(label, quote=True)}" title="{escape(label, quote=True)}"'
        if icon_only
        else ""
    )
    state = f' aria-pressed="{str(pressed).lower()}"' if pressed is not None else ""
    content = (icon(symbol) if symbol else "") + ("" if icon_only else escape(label))
    return (
        f'<button type="button" class="{classes}" data-action="{escape(name)}"'
        f' data-value="{escape(encoded, quote=True)}"{accessible}{state}>{content}</button>'
    )


def ticking(zone: str, fmt: str, lang: str, *, seconds: bool = True) -> str:
    """A span the shell keeps ticking — no re-render needed to show time."""
    return (
        f'<span data-clock-text="{escape(zone, quote=True)}" data-clock-format="{fmt}"'
        f' data-clock-lang="{escape(lang, quote=True)}"'
        f' data-clock-seconds="{"true" if seconds else "false"}"></span>'
    )


def two_lines(title: str, sub: str) -> str:
    return f"<span>{escape(title)}<br><small class='sb-faint'>{escape(sub)}</small></span>"


# --- tile ----------------------------------------------------------------


def tile(zone: str, face: str, seconds: bool, lang: str) -> str:
    if face == "analog":
        inner = (
            f'<span data-clock="{escape(zone, quote=True)}"></span>'
            f'<span class="sb-tile-stack"><span>{ticking(zone, "weekday", lang)}</span>'
            f'<span class="sb-accent">{ticking(zone, "date", lang)}</span></span>'
        )
    else:
        inner = (
            f'<span class="sb-tile-stack"><span class="sb-mono">{ticking(zone, "time", lang, seconds=seconds)}</span>'
            f'<span class="sb-accent">{ticking(zone, "date", lang)}</span></span>'
        )
    return f'<div class="sb-tile">{inner}</div>'


# --- hover ---------------------------------------------------------------


def fact_rows(zone: str, lang: str, t: Translator) -> str:
    rows = (
        ("clock.weekday", ticking(zone, "weekday", lang), ""),
        ("clock.date", ticking(zone, "date", lang), ""),
        ("clock.week", ticking(zone, "week", lang), "sb-mono"),
        ("clock.offset", ticking(zone, "offset", lang), "sb-mono"),
    )
    return "".join(
        f'<div class="sb-row"><span>{escape(t(key))}</span><span class="sb-push {cls}">{value}</span></div>'
        for key, value, cls in rows
    )


def hover(home: dict, others: list[dict], lang: str, t: Translator) -> str:
    html = (
        f'<div class="sb-header"><span class="sb-title">{escape(home["label"])}</span></div>'
        f'<div class="sb-list">{fact_rows(home["zone"], lang, t)}</div>'
    )
    if others:
        rows = "".join(
            f'<div class="sb-row">{icon(clock["phase_icon"], t(clock["phase_key"]))}'
            f'<span>{escape(clock["label"])}</span>'
            f'<span class="sb-mono sb-push">{ticking(clock["zone"], "time", lang, seconds=False)}</span></div>'
            for clock in others
        )
        html += f'<div class="sb-section">{escape(t("clock.worldClocks"))}</div><div class="sb-list">{rows}</div>'
    return html


# --- flyout --------------------------------------------------------------


def header(home: dict, facts: dict, face: str, t: Translator) -> str:
    to_analog = face != "analog"
    return (
        f'<div class="sb-header"><span class="sb-icon-badge">{icon("clock")}</span>'
        f'<div><h2 class="sb-title">{escape(t("clock.title"))}</h2>'
        f'<p class="sb-meta">{escape(home["label"])} · {escape(facts["abbr"])}</p></div>'
        '<div class="sb-header-actions">'
        + action(
            "face",
            t("clock.faceAnalog" if to_analog else "clock.faceDigital"),
            symbol="clock",
            icon_only=True,
            pressed=not to_analog,
            ghost=False,
        )
        + action("seconds", t("clock.toggleSeconds"), symbol="timer", icon_only=True, ghost=False)
        + "</div></div>"
    )


def home_card(home: dict, seconds: bool, lang: str) -> str:
    zone = home["zone"]
    return (
        '<div class="sb-card sb-center">'
        f'<div data-clock="{escape(zone, quote=True)}"></div>'
        f'<div class="sb-mono sb-text-xl">{ticking(zone, "time", lang, seconds=seconds)}</div>'
        f'<div class="sb-dim">{ticking(zone, "weekday", lang)}, {ticking(zone, "date", lang)}'
        f' · {ticking(zone, "offset", lang)}</div></div>'
    )


def facts_block(home: dict, facts: dict, lang: str, t: Translator) -> str:
    zone = home["zone"]
    kpis = (
        f'<div class="sb-kpi"><span class="sb-kpi-value sb-mono">{ticking(zone, "week", lang)}</span>'
        f'<span class="sb-kpi-label">{escape(t("clock.week"))}</span></div>'
        f'<div class="sb-kpi"><span class="sb-kpi-value sb-mono">{facts["day_of_year"]}'
        f'<small class="sb-faint">/{facts["days_in_year"]}</small></span>'
        f'<span class="sb-kpi-label">{escape(t("clock.dayOfYear"))}</span></div>'
    )
    rows = [
        f'<div class="sb-row">{icon("globe")}'
        + two_lines(f'{facts["abbr"]} · {facts["offset"]}', facts["dst"])
        + "</div>"
    ]
    if facts["sun"] is not None:
        rise, set_, daylight = facts["sun"]
        rows.append(
            f'<div class="sb-row">{icon("sunrise")}'
            + two_lines(f"{rise} – {set_}", t("clock.daylight").replace("{hours}", daylight))
            + "</div>"
        )
    rows.append(
        f'<div class="sb-row">{icon("terminal")}'
        f'<span>{escape(t("clock.unix"))}<br><small class="sb-faint sb-mono" id="clock-unix">'
        f'{ticking(zone, "unix", lang)}</small></span>'
        f'<button type="button" class="sb-btn sb-btn--icon sb-copy sb-push" data-sb-copy="#clock-unix"'
        f' aria-label="{escape(t("clock.copyUnix"), quote=True)}" title="{escape(t("clock.copyUnix"), quote=True)}"></button></div>'
    )
    return f'<div class="sb-kpi-grid">{kpis}</div><div class="sb-list">{"".join(rows)}</div>'


def world_clock_row(index: int, clock: dict, lang: str, t: Translator) -> str:
    menu = [
        {"action": "primary", "value": str(index), "label": t("clock.showOnBar"), "icon": "star"},
        {"separator": True},
        {"action": "remove", "value": str(index), "label": t("clock.remove"), "icon": "trash-2", "danger": True},
    ]
    sub = " · ".join(part for part in (clock["region"], clock["relation"]) if part)
    return (
        f'<div class="sb-row" data-context-items="{escape(json.dumps(menu, separators=(",", ":")), quote=True)}">'
        f'{icon(clock["phase_icon"], t(clock["phase_key"]))}'
        + two_lines(clock["label"], sub)
        + f'<span class="sb-mono sb-push">{ticking(clock["zone"], "time", lang, seconds=False)}</span>'
        + action("remove", t("clock.remove"), str(index), symbol="x", icon_only=True)
        + "</div>"
    )


def world_clocks(others: list[tuple[int, dict]], lang: str, t: Translator) -> str:
    section = f'<div class="sb-section">{escape(t("clock.worldClocks"))}</div>'
    if not others:
        return (
            section + f'<div class="sb-empty"><div class="sb-empty__icon sb-accent">{icon("globe")}</div>'
            f'<p class="sb-empty__title">{escape(t("clock.emptyTitle"))}</p>'
            f'<p class="sb-empty__text">{escape(t("clock.emptyHint"))}</p></div>'
        )
    rows = "".join(world_clock_row(index, clock, lang, t) for index, clock in others)
    return f'{section}<div class="sb-list">{rows}</div>'


def calendar(today: date, month_name: str, t: Translator) -> str:
    """Month grid with ISO weeks in the first column and today marked."""
    first = today.replace(day=1)
    next_month = (first + timedelta(days=32)).replace(day=1)
    heads = [f'<span class="sb-faint">{escape(t("clock.weekShort"))}</span>'] + [
        f'<span class="sb-faint">{escape(name)}</span>' for name in t("clock.weekdayInitials").split(",")
    ]
    cells: list[str] = []
    day = first - timedelta(days=first.weekday())
    while day < next_month:
        cells.append(f'<span class="sb-faint sb-text-xs">{day.isocalendar().week}</span>')
        for _ in range(7):
            if day.month != today.month:
                cells.append("<span></span>")
            elif day == today:
                cells.append(f'<span class="sb-badge sb-badge-accent">{day.day}</span>')
            else:
                cells.append(f"<span>{day.day}</span>")
            day += timedelta(days=1)
    return (
        f'<div class="sb-section">{escape(month_name)}</div>'
        '<div class="sb-grid sb-center sb-text-s" style="--sb-grid-cols:8">'
        f'{"".join(heads)}{"".join(cells)}</div>'
    )


def result_row(place: dict, offset: str, t: Translator) -> str:
    sub = " · ".join(part for part in (place["region"], place["zone"], offset) if part)
    return (
        f'<div class="sb-row">{icon("map-pin")}'
        + two_lines(place["label"], sub)
        + '<span class="sb-push"></span>'
        + action("add", t("clock.add"), place, symbol="plus", icon_only=True)
        + "</div>"
    )


def search(query: str, results: list[tuple[dict, str]], failed: bool, t: Translator) -> str:
    html = (
        '<form class="sb-stack"><div class="sb-field-stack">'
        f'<label class="sb-field__label" for="clock-query">{escape(t("clock.addZone"))}</label>'
        '<div class="sb-field">'
        f'<input class="sb-input" id="clock-query" data-field="query" type="search" value="{escape(query, quote=True)}"'
        f' placeholder="{escape(t("clock.searchPlaceholder"), quote=True)}" aria-describedby="clock-query-hint">'
        f'<button type="submit" class="sb-btn sb-btn-primary" data-action="search">{icon("search")}{escape(t("clock.search"))}</button>'
        "</div>"
        f'<p class="sb-field__hint" id="clock-query-hint">{escape(t("clock.searchHint"))}</p>'
        "</div></form>"
    )
    if failed:
        html += (
            f'<div class="sb-alert sb-alert--warn" role="alert"><span class="sb-alert__icon">{icon("wifi-off")}</span>'
            f'<div><p class="sb-alert__title">{escape(t("clock.searchOffline"))}</p>'
            f'<p class="sb-alert__text">{escape(t("clock.searchOfflineHint"))}</p></div></div>'
        )
    rows = "".join(result_row(place, offset, t) for place, offset in results)
    html += f'<div class="sb-reveal{" sb-active" if rows else ""}"><div class="sb-list">{rows}</div></div>'
    if query and not rows and not failed:
        html += f'<p class="sb-error">{escape(t("clock.noMatch"))}</p>'
    return html


def flyout(
    home: dict,
    facts: dict,
    others: list[tuple[int, dict]],
    *,
    face: str,
    seconds: bool,
    lang: str,
    calendar_html: str,
    search_html: str,
    t: Translator,
) -> str:
    return (
        header(home, facts, face, t)
        + home_card(home, seconds, lang)
        + facts_block(home, facts, lang, t)
        + world_clocks(others, lang, t)
        + calendar_html
        + search_html
    )


def relation_text(minutes: int, day_delta: int, t: Translator) -> str:
    """"+8 h · tomorrow" relative to the home clock; empty for the same time."""
    parts: list[str] = []
    if minutes:
        hours, rest = divmod(abs(minutes), 60)
        span = f"{hours}:{rest:02d}" if rest else str(hours)
        parts.append(f"{'+' if minutes > 0 else '−'}{span} h")
    if day_delta > 0:
        parts.append(t("clock.tomorrow"))
    elif day_delta < 0:
        parts.append(t("clock.yesterday"))
    return " · ".join(parts)


def short_date(day: datetime | date, t: Translator) -> str:
    months = t("clock.monthsShort").split(",")
    month = months[day.month - 1] if len(months) == 12 else day.strftime("%b")
    return t("clock.dateShort").replace("{d}", str(day.day)).replace("{m}", month)
