"""Weather markup: semantic HTML on the smabar UI kit, no styling of its own.

The flyout is one `data-tabs` container: the place list rows are the tabs, the
detail panels their targets, so switching places never round-trips to the
plugin. Local times tick through `data-clock-text` with the place's zone.
"""

import json
from collections.abc import Callable
from datetime import date
from html import escape

Translator = Callable[[str], str]
NO_DATA = "–"
WIND_UNIT = {"metric": "km/h", "imperial": "mph"}

# WMO weather interpretation codes (open-meteo.com/en/docs) → lucide icon + locale key.
_WMO_GROUPS: list[tuple[tuple[int, ...], str, str]] = [
    ((0,), "sun", "clear"),
    ((1,), "cloud-sun", "mostly_clear"),
    ((2,), "cloud-sun", "partly_cloudy"),
    ((3,), "cloud", "overcast"),
    ((45, 48), "cloud-fog", "fog"),
    ((51, 53, 55), "cloud-drizzle", "drizzle"),
    ((56, 57), "cloud-drizzle", "freezing_drizzle"),
    ((61, 63, 65), "cloud-rain", "rain"),
    ((66, 67), "cloud-rain", "freezing_rain"),
    ((71, 73, 75), "cloud-snow", "snow"),
    ((77,), "snowflake", "snow_grains"),
    ((80, 81, 82), "cloud-rain", "showers"),
    ((85, 86), "cloud-snow", "snow_showers"),
    ((95,), "cloud-lightning", "thunderstorm"),
    ((96, 99), "cloud-lightning", "thunderstorm_hail"),
]
WMO = {code: (icon, f"weather.code.{key}") for codes, icon, key in _WMO_GROUPS for code in codes}
NIGHT_ICONS = {"sun": "moon", "cloud-sun": "cloud-moon"}
RAINY_CODES = {51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 80, 81, 82, 95, 96, 99}
DAY_KEYS = ("weather.day.mon", "weather.day.tue", "weather.day.wed", "weather.day.thu",
            "weather.day.fri", "weather.day.sat", "weather.day.sun")


def wmo(code: int, is_day: bool = True) -> tuple[str, str]:
    """Lucide icon + locale key; unknown codes get a fallback, nights a moon."""
    icon_name, key = WMO.get(code, ("thermometer", "weather.code.unknown"))
    return (icon_name if is_day else NIGHT_ICONS.get(icon_name, icon_name)), key


def icon(name: str, title: str = "") -> str:
    hint = f' title="{escape(title, quote=True)}"' if title else ""
    return f'<span data-lucide="{name}" aria-hidden="true"{hint}></span>'


def action(name: str, label: str, value: object = "", *, primary: bool = False,
           symbol: str = "", icon_only: bool = False, small: bool = False) -> str:
    encoded = value if isinstance(value, str) else json.dumps(value, separators=(",", ":"))
    classes = "sb-btn sb-btn-primary" if primary else "sb-btn sb-btn-ghost"
    if icon_only:
        classes += " sb-btn-icon"
    if small:
        classes += " sb-btn--sm"
    accessible = (f' aria-label="{escape(label, quote=True)}" title="{escape(label, quote=True)}"'
                  if icon_only else "")
    content = (icon(symbol) if symbol else "") + ("" if icon_only else escape(label))
    return (f'<button type="button" class="{classes}" data-action="{escape(name)}"'
            f' data-value="{escape(encoded, quote=True)}"{accessible}>{content}</button>')


def local_time(zone: str, lang: str) -> str:
    return (f'<span data-clock-text="{escape(zone, quote=True)}" data-clock-lang="{escape(lang, quote=True)}"'
            ' data-clock-seconds="false"></span>')


def two_lines(title: str, sub: str) -> str:
    return f"<span>{escape(title)}<br><small class='sb-faint'>{sub}</small></span>"


def degrees(value: float | None, decimals: int = 0) -> str:
    return NO_DATA if value is None else f"{value:.{decimals}f}°"


def percent(value: float | None) -> str:
    return NO_DATA if value is None else f"{value:.0f} %"


def day_label(index: int, iso_day: str, t: Translator) -> str:
    if index == 0:
        return t("weather.today")
    try:
        return t(DAY_KEYS[date.fromisoformat(iso_day).weekday()])
    except ValueError:
        return iso_day[-2:]


# --- tile ----------------------------------------------------------------


def tile_view(place: dict, layout: str, t: Translator) -> str:
    data = place["data"]
    if data is None:
        return (f'<div class="sb-inline">{icon("thermometer")}'
                f'<span class="sb-muted">{escape(t("weather.no_data"))}</span></div>')
    icon_name, desc = wmo(data["code"], data["is_day"])
    if layout == "basicPlus":
        return (f'<div class="sb-inline">{icon(icon_name)}<span class="sb-tile-stack">'
                f'<span>{degrees(data["temp"])}</span><span class="sb-muted">{escape(place["name"])}</span>'
                "</span></div>")
    return (f'<div class="sb-inline"><span class="sb-tile-seg"><span>{escape(place["name"])}</span>'
            f'<span>{degrees(data["temp"])} {escape(t(desc))}</span></span></div>')


def tile(places: list[dict], layout: str, rotates: bool, error: str, t: Translator) -> str:
    warn = icon("triangle-alert", t("weather.fetch_failed")).replace('aria-hidden="true"',
                                                                        'aria-hidden="true" class="sb-warn"') if error else ""
    if not places:
        inner = f'{icon("map-pin")}<span class="sb-muted">{escape(t("weather.addPlace"))}</span>'
        title = t("weather.addPlace")
    else:
        shown = places if rotates else places[:1]
        views = "".join(tile_view(place, layout, t) for place in shown)
        if len(shown) > 1:
            views = f'<div data-rotator="up" data-rotator-interval="5000">{views}</div>'
        inner = views + warn
        title = t("weather.fetch_failed") if error else places[0]["name"]
    return f'<div class="sb-tile" title="{escape(title, quote=True)}">{inner}</div>'


# --- hover ---------------------------------------------------------------


def summary(place: dict, lang: str) -> tuple[str, str]:
    """(temperature, hi/lo) as inline markup for list rows."""
    data = place["data"]
    if data is None:
        return f'<span class="sb-badge">{NO_DATA}</span>', ""
    today = data["days"][0] if data["days"] else None
    hi_lo = f'{degrees(today["tmax"])} / {degrees(today["tmin"])}' if today else ""
    return f'<span class="sb-mono">{degrees(data["temp"])}</span>', hi_lo


def hover(places: list[dict], lang: str, t: Translator) -> str:
    if not places:
        return f'<div class="sb-card"><p class="sb-dim">{escape(t("weather.addPlace"))}</p></div>'
    rows = []
    for place in places:
        data = place["data"]
        icon_name = wmo(data["code"], data["is_day"])[0] if data else "thermometer"
        temp, hi_lo = summary(place, lang)
        rows.append(f'<div class="sb-row">{icon(icon_name)}'
                    + two_lines(place["name"], local_time(place["timezone"], lang))
                    + f'<span class="sb-push">{temp} <small class="sb-faint">{hi_lo}</small></span></div>')
    return (f'<div class="sb-section">{escape(t("weather.title"))}</div>'
            f'<div class="sb-list">{"".join(rows)}</div>'
            f'<p class="sb-meta">{escape(t("weather.hover_hint"))}</p>')


# --- flyout --------------------------------------------------------------


def header(places: list[dict], selected: int, t: Translator) -> str:
    current = places[selected]["data"] if places else None
    icon_name = wmo(current["code"], current["is_day"])[0] if current else "cloud-sun"
    count = (t("weather.place_one") if len(places) == 1
             else t("weather.places").replace("{n}", str(len(places))))
    return (f'<div class="sb-header"><span class="sb-icon-badge">{icon(icon_name)}</span>'
            f'<div><h2 class="sb-title">{escape(t("weather.title"))}</h2><p class="sb-meta">{escape(count)}</p></div>'
            '<div class="sb-header-actions">'
            + action("refresh", t("weather.refresh"), symbol="refresh-cw", icon_only=True)
            + "</div></div>")


def place_row(index: int, place: dict, selected: bool, lang: str, t: Translator) -> str:
    data = place["data"]
    icon_name = wmo(data["code"], data["is_day"])[0] if data else "thermometer"
    temp, hi_lo = summary(place, lang)
    # Only the ticking local time under the name: the region would wrap the row,
    # and the panel's hero names the place in full.
    sub = local_time(place["timezone"], lang)
    # The row is the tab (any [data-tab] element switches panels) and carries its
    # small text actions next to the local time: the favorite owns the bar tile
    # and wears a star, every other place offers "make favorite"; all can go.
    if index == 0:
        star = f' <span class="sb-accent">{icon("star", t("weather.default"))}</span>'
        favorite = f'<small class="sb-faint">{escape(t("weather.default"))}</small>'
    else:
        star = ""
        favorite = action("primary", t("weather.makeDefault"), str(index), small=True)
    remove = action("remove", t("weather.remove"), str(index), small=True)
    return (f'<div class="sb-row{" sb-active" if selected else ""}" data-tab="p{index}"'
            f' data-action="select" data-value="{index}">'
            f'{icon(icon_name)}<span class="sb-wrap">{escape(place["name"])}{star}<br>'
            f'<small class="sb-faint">{sub} ·</small> {favorite} <small class="sb-faint">·</small> {remove}</span>'
            f'<span class="sb-push">{temp} <small class="sb-faint">{hi_lo}</small></span></div>')


def kpi(value: str, label: str) -> str:
    return (f'<div class="sb-kpi"><span class="sb-kpi-value">{value}</span>'
            f'<span class="sb-kpi-label">{escape(label)}</span></div>')


def hero(index: int, place: dict, units: str, t: Translator) -> str:
    data = place["data"]
    desc = t(wmo(data["code"], data["is_day"])[1])
    where = " · ".join(part for part in (place["name"], place["region"]) if part)
    return (f'<div class="sb-hero"><small>{escape(where)}</small>'
            f'<h1 data-sb-tween data-sb-key="temp-{index}">{degrees(data["temp"], 1)}</h1>'
            f'<p class="sb-text-s">{escape(desc)} · {escape(t("weather.feels_like"))} {degrees(data["feels"], 1)}'
            f' · {icon("wind")} {data["wind"]:.0f} {WIND_UNIT[units]}</p></div>')


def kpis(data: dict, units: str, t: Translator) -> str:
    today = data["days"][0] if data["days"] else {}
    gusts = f' · {t("weather.gusts")} {data["gusts"]:.0f}' if data["gusts"] is not None else ""
    cells = [
        kpi(percent(data["humidity"]), t("weather.humidity")),
        kpi(f'{data["wind"]:.0f} {WIND_UNIT[units]}', t("weather.wind") + gusts),
        kpi(f'{data["pressure"]:.0f} hPa' if data["pressure"] is not None else NO_DATA, t("weather.pressure")),
        kpi(percent(data["cloud"]), t("weather.cloud_cover")),
        kpi(f'{today["uv"]:.1f}' if today.get("uv") is not None else NO_DATA, t("weather.uv")),
        kpi(percent(today.get("rain")), t("weather.rain_today")),
        kpi(escape(today.get("sunrise") or NO_DATA), t("weather.sunrise")),
        kpi(escape(today.get("sunset") or NO_DATA), t("weather.sunset")),
    ]
    return f'<div class="sb-kpi-grid">{"".join(cells)}</div>'


def next_hours(index: int, data: dict, t: Translator) -> str:
    hours = data["hours"]
    if len(hours) < 2:
        return ""
    temps = [temp for _hour, temp, _rain in hours]
    points = ",".join(f"{temp:.1f}" for temp in temps)
    # Six columns (every fourth hour) fit the 340px flyout without a horizontal
    # scroll; the kit's column minimum would push eight past the edge.
    columns = hours[::4][:6]
    # The title becomes the shell's tooltip: a bare column says nothing about
    # what its height measures.
    bars = "".join(
        f'<div class="sb-chart-bars__item{" sb-chart-bars__item--accent" if rain == max(r for _h, _t, r in columns) and rain > 0 else ""}"'
        f' style="--sb-chart-value: {rain:.0f}"'
        f' title="{escape(t("weather.rain_at").replace("{hour}", hour).replace("{percent}", f"{rain:.0f}"), quote=True)}">'
        f'<span class="sb-chart-bars__bar">'
        f'<span class="sb-chart-bars__value">{rain:.0f}</span></span>'
        f'<span class="sb-chart-bars__label">{escape(hour)}</span></div>'
        for hour, _temp, rain in columns)
    return (f'<div class="sb-section">{escape(t("weather.next_hours"))}</div>'
            f'<div class="sb-spark" data-chart="sparkline" data-points="{points}" data-sb-key="spark-{index}"></div>'
            f'<small>{degrees(min(temps))} {NO_DATA} {degrees(max(temps))}</small>'
            f'<div class="sb-section">{escape(t("weather.rain_chance"))}</div>'
            f'<div class="sb-chart-bars" role="img" aria-label="{escape(t("weather.rain_chance"), quote=True)}"'
            f' style="--sb-chart-max: 100">{bars}</div>')


def forecast(data: dict, t: Translator) -> str:
    rows = []
    for index, day in enumerate(data["days"]):
        icon_name, desc = wmo(day["code"])
        rain = f'<span class="sb-faint">{icon("droplets")} {percent(day["rain"])}</span>' if day["rain"] is not None else ""
        rows.append(f'<div class="sb-row" title="{escape(t(desc), quote=True)}">{icon(icon_name)}'
                    f'<span>{escape(day_label(index, day["date"], t))}</span>{rain}'
                    f'<span class="sb-mono sb-push"><b>{degrees(day["tmax"])}</b>'
                    f' <span class="sb-muted">{degrees(day["tmin"])}</span></span></div>')
    return (f'<div class="sb-section">{escape(t("weather.forecast"))}</div>'
            f'<div class="sb-list">{"".join(rows)}</div>')


def tip(data: dict, t: Translator) -> str:
    """One actionable hint: umbrella on rain, frost warning, sunscreen on high UV."""
    if not data["days"]:
        return ""
    today = data["days"][0]
    if today["code"] in RAINY_CODES or (today["rain"] or 0) >= 60:
        icon_name, text = "umbrella", t("weather.tip.umbrella")
    elif today["tmin"] <= 0:
        icon_name, text = "snowflake", t("weather.tip.frost")
    elif (today["uv"] or 0) >= 6:
        icon_name, text = "sun", t("weather.tip.uv")
    else:
        return ""
    return f'<div class="sb-tip">{icon(icon_name)}<span>{escape(text)}</span></div>'


def panel(index: int, place: dict, selected: bool, units: str, t: Translator) -> str:
    data = place["data"]
    if data is None:
        body = (f'<div class="sb-card"><p class="sb-dim">{escape(t("weather.no_data"))}</p>'
                f'<small>{escape(t("weather.offline_hint"))}</small></div>')
    else:
        body = hero(index, place, units, t) + kpis(data, units, t) + next_hours(index, data, t) \
            + forecast(data, t) + tip(data, t)
    return f'<div data-tab-panel="p{index}"{"" if selected else " hidden"}>{body}</div>'


def result_row(place: dict, t: Translator) -> str:
    sub = " · ".join(part for part in (place["region"], place["timezone"]) if part)
    return (f'<div class="sb-row">{icon("map-pin")}' + two_lines(place["name"], escape(sub))
            + '<span class="sb-push"></span>'
            + action("add", t("weather.add"), place, symbol="plus", icon_only=True) + "</div>")


def search(query: str, results: list[dict], failed: bool, t: Translator) -> str:
    html = ('<form class="sb-stack"><div class="sb-field-stack">'
            f'<label class="sb-field__label" for="weather-query">{escape(t("weather.addPlace"))}</label>'
            '<div class="sb-field">'
            f'<input class="sb-input" id="weather-query" data-field="query" type="search" value="{escape(query, quote=True)}"'
            f' placeholder="{escape(t("weather.search_placeholder"), quote=True)}" aria-describedby="weather-query-hint">'
            f'<button type="submit" class="sb-btn sb-btn-primary" data-action="search">{icon("search")}{escape(t("weather.search"))}</button>'
            "</div>"
            f'<p class="sb-field__hint" id="weather-query-hint">{escape(t("weather.search_hint"))}</p>'
            "</div></form>")
    if failed:
        html += (f'<div class="sb-alert sb-alert--warn" role="alert"><span class="sb-alert__icon">{icon("wifi-off")}</span>'
                 f'<div><p class="sb-alert__title">{escape(t("weather.search_failed"))}</p>'
                 f'<p class="sb-alert__text">{escape(t("weather.offline_hint"))}</p></div></div>')
    rows = "".join(result_row(place, t) for place in results)
    html += f'<div class="sb-reveal{" sb-active" if rows else ""}"><div class="sb-list">{rows}</div></div>'
    if query and not rows and not failed:
        html += f'<p class="sb-error">{escape(t("weather.no_results"))}</p>'
    return html


def footer(error: str, last_ok: str, t: Translator) -> str:
    if error:
        return (f'<div class="sb-alert sb-alert--warn" role="alert"><span class="sb-alert__icon">{icon("triangle-alert")}</span>'
                f'<div><p class="sb-alert__title">{escape(t("weather.fetch_failed"))}</p>'
                f'<p class="sb-alert__text">{escape(error[:120])} · {escape(t("weather.last_success"))}: {escape(last_ok or NO_DATA)}</p></div></div>')
    return f'<p class="sb-meta">{escape(t("weather.updated"))} {escape(last_ok or NO_DATA)} · Open-Meteo</p>'


def empty(t: Translator) -> str:
    return (f'<div class="sb-empty"><div class="sb-empty__icon sb-accent">{icon("map-pin")}</div>'
            f'<p class="sb-empty__title">{escape(t("weather.emptyTitle"))}</p>'
            f'<p class="sb-empty__text">{escape(t("weather.emptyHint"))}</p></div>')


def flyout(places: list[dict], selected: int, *, units: str, lang: str, error: str, last_ok: str,
           search_html: str, t: Translator) -> str:
    if not places:
        body = empty(t)
    else:
        rows = "".join(place_row(i, place, i == selected, lang, t) for i, place in enumerate(places))
        panels = "".join(panel(i, place, i == selected, units, t) for i, place in enumerate(places))
        body = f'<div data-tabs><div class="sb-list">{rows}</div>{panels}</div>'
    return header(places, selected, t) + body + search_html + footer(error, last_ok, t)
