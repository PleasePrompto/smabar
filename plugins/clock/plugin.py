# /// script
# requires-python = ">=3.12"
# dependencies = ["tzdata==2026.3"]
# ///
"""clock — time, date, world clocks and the facts around them.

Renders ONCE per settings change and then idles: the ticking is the shell's
job (`data-clock-text` for text, `data-clock` for the analog face), so the
plugin process stays asleep. It wakes once a minute only to notice that a
zone's hour or date rolled over — the moment the day-phase icons, the
"tomorrow" badges and the calendar change.

Places come from Open-Meteo's keyless geocoding API, which answers any city or
country name in the user's language with its IANA time zone and coordinates.
The coordinates feed the sunrise/sunset row (solar.py, computed offline).
"""

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, date, datetime, timedelta
from zoneinfo import ZoneInfo, available_timezones

from smabar_sdk import Plugin
from solar import sun_times
import views

app = Plugin()
TILE = "clock"
GEOCODE_API = "https://geocoding-api.open-meteo.com/v1/search"
MAX_RESULTS = 6
FETCH_ERRORS = (urllib.error.URLError, TimeoutError, ValueError, TypeError)
SYSTEM_CLOCK = {"label": "", "zone": "", "region": "", "latitude": None, "longitude": None}
# Local hour → (lucide icon, locale key): "can I call them right now?" at a glance.
PHASES = ((6, "moon", "clock.phase.night"), (9, "sunrise", "clock.phase.morning"),
          (18, "sun", "clock.phase.day"), (22, "sunset", "clock.phase.evening"))

# The search form plus the hourly signature that decides whether a redraw is due.
state: dict = {"query": "", "results": [], "search_failed": False, "signature": ()}


# --- settings ------------------------------------------------------------


def flag(name: str, default: bool) -> bool:
    value = app.settings.get(name)
    return value if isinstance(value, bool) else default


def face() -> str:
    return "analog" if app.settings.get("face") == "analog" else "digital"


def valid_zone(zone: str) -> bool:
    if zone == "":
        return True
    try:
        ZoneInfo(zone)
    except (KeyError, ValueError, OSError):
        return False
    return True


def normalize(raw: object) -> dict | None:
    """One clock from settings or a search result; None when unusable."""
    if not isinstance(raw, dict):
        return None
    label, zone = raw.get("label"), raw.get("zone")
    if not isinstance(label, str) or not isinstance(zone, str) or not valid_zone(zone):
        app.log("warn", "ignoring a clock with an unknown time zone", zone=repr(zone),
                hint="use an IANA id such as Europe/Berlin, or search the place again")
        return None
    lat, lon = raw.get("latitude"), raw.get("longitude")
    coords = all(isinstance(v, int | float) and not isinstance(v, bool) for v in (lat, lon))
    return {
        "label": label,
        "zone": zone,
        "region": str(raw.get("region") or ""),
        "latitude": float(lat) if coords else None,
        "longitude": float(lon) if coords else None,
    }


def clocks() -> list[dict]:
    """Configured clocks; the first one owns the tile. Empty = the system clock."""
    raw = app.settings.get("clocks")
    picked = [c for c in map(normalize, raw) if c is not None] if isinstance(raw, list) else []
    return picked or [dict(SYSTEM_CLOCK)]


def save_clocks(picked: list[dict]) -> None:
    """Persist; settings.set REPLACES the whole object, so carry the other keys."""
    stored = [{k: v for k, v in clock.items() if k in ("label", "zone") or v not in ("", None)}
              for clock in picked]
    app.set_settings({**app.settings, "clocks": stored})


def save(patch: dict[str, object]) -> None:
    app.set_settings({**app.settings, **patch})


# --- time facts ----------------------------------------------------------


def now_in(zone: str) -> datetime:
    return datetime.now(ZoneInfo(zone)) if zone else datetime.now().astimezone()


def system_zone() -> str:
    """IANA name of the system zone — Linux only, via the /etc/localtime link."""
    # ponytail: Windows keeps an empty name, so its DST forecast row stays blank.
    try:
        target = os.readlink("/etc/localtime")
    except OSError:
        return ""
    marker = "zoneinfo/"
    return target.split(marker, 1)[1] if marker in target else ""


def next_transition(zone: str, now: datetime) -> datetime | None:
    """The next UTC-offset change within 400 days, to the hour."""
    tz = ZoneInfo(zone)
    offset = now.astimezone(tz).utcoffset()
    probe = now.astimezone(UTC).replace(minute=0, second=0, microsecond=0)
    for _ in range(24 * 400):
        probe += timedelta(hours=1)
        if probe.astimezone(tz).utcoffset() != offset:
            return probe.astimezone(tz)
    return None


def dst_text(zone: str, now: datetime) -> str:
    """"Summer time · standard time from 25 Oct" — or that the zone has none."""
    named = zone or system_zone()
    if not named:
        return app.t("clock.dst.unknown")
    change = next_transition(named, now)
    if change is None:
        return app.t("clock.dst.none")
    # The system clock carries a fixed offset without DST knowledge; ask the named zone.
    summer = bool(now.astimezone(ZoneInfo(named)).dst())
    current = app.t("clock.dst.summer" if summer else "clock.dst.standard")
    upcoming = app.t("clock.dst.standard" if summer else "clock.dst.summer")
    return f"{current} · " + app.t("clock.dst.next").replace("{name}", upcoming).replace(
        "{date}", views.short_date(change, app.t))


def sun_text(clock: dict, now: datetime) -> tuple[str, str, str] | None:
    if clock["latitude"] is None:
        return None
    times = sun_times(now.date(), clock["latitude"], clock["longitude"])
    if times is None:
        return None
    rise, set_ = (moment.astimezone(now.tzinfo) for moment in times)
    minutes = int((set_ - rise).total_seconds() // 60)
    return rise.strftime("%H:%M"), set_.strftime("%H:%M"), f"{minutes // 60} h {minutes % 60:02d} min"


def utc_label(now: datetime) -> str:
    minutes = int((now.utcoffset() or timedelta()).total_seconds() // 60)
    if minutes == 0:
        return "UTC±0"
    hours, rest = divmod(abs(minutes), 60)
    return f"UTC{'+' if minutes > 0 else '−'}{hours}" + (f":{rest:02d}" if rest else "")


def home_facts(home: dict) -> dict:
    now = now_in(home["zone"])
    return {
        "abbr": now.tzname() or "",
        "offset": utc_label(now),
        "dst": dst_text(home["zone"], now),
        "sun": sun_text(home, now),
        "day_of_year": now.timetuple().tm_yday,
        "days_in_year": date(now.year, 12, 31).timetuple().tm_yday,
    }


def with_phase(clock: dict, home_now: datetime) -> dict:
    """The clock plus its day phase and its relation to the home clock."""
    now = now_in(clock["zone"])
    phase_icon, phase_key = "moon", "clock.phase.night"
    for limit, icon_name, key in PHASES:
        if now.hour < limit:
            phase_icon, phase_key = icon_name, key
            break
    minutes = int(((now.utcoffset() or timedelta()) - (home_now.utcoffset() or timedelta()))
                  .total_seconds() // 60)
    return {
        **clock,
        "phase_icon": phase_icon,
        "phase_key": phase_key,
        "relation": views.relation_text(minutes, (now.date() - home_now.date()).days, app.t),
    }


def display(clock: dict) -> dict:
    return {**clock, "label": clock["label"] or app.t("clock.systemZone")}


def signature() -> tuple:
    return tuple((c["zone"], now_in(c["zone"]).strftime("%Y-%m-%d %H")) for c in clocks())


# --- place search --------------------------------------------------------


def geocode(query: str) -> list[dict]:
    """Places matching `query` from Open-Meteo, in the user's language."""
    params = urllib.parse.urlencode(
        {"name": query, "count": MAX_RESULTS, "language": app.language, "format": "json"})
    with urllib.request.urlopen(f"{GEOCODE_API}?{params}", timeout=8) as response:
        payload = json.load(response)
    results = payload.get("results", []) if isinstance(payload, dict) else None
    if not isinstance(results, list):
        raise TypeError("unexpected geocoding response shape")
    places = []
    for item in results:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        region = ", ".join(str(item[key]) for key in ("admin1", "country")
                           if isinstance(item.get(key), str) and item[key] != name)
        place = normalize({"label": name, "zone": item.get("timezone"), "region": region,
                           "latitude": item.get("latitude"), "longitude": item.get("longitude")})
        if place is not None:
            places.append(place)
    return places


def zone_matches(query: str) -> list[dict]:
    """Offline fallback: IANA zone names containing the query."""
    needle = query.strip().lower().replace(" ", "_")
    hits = sorted(z for z in available_timezones() if needle in z.lower())[:MAX_RESULTS]
    return [{"label": z.rsplit("/", 1)[-1].replace("_", " "), "zone": z, "region": "",
             "latitude": None, "longitude": None} for z in hits]


# --- rendering -----------------------------------------------------------


def render_all() -> None:
    picked = clocks()
    home = display(picked[0])
    home_now = now_in(home["zone"])
    others = [(index, with_phase(display(clock), home_now)) for index, clock in enumerate(picked) if index > 0]
    seconds = flag("showSeconds", True)
    app.render(TILE, "tile", views.tile(home["zone"], face(), seconds, app.language))
    app.render(TILE, "hover", views.hover(home, [clock for _i, clock in others], app.language, app.t))
    calendar_html = ""
    if flag("showCalendar", True):
        months = app.t("clock.months").split(",")
        month = months[home_now.month - 1] if len(months) == 12 else home_now.strftime("%B")
        calendar_html = views.calendar(home_now.date(), f"{month} {home_now.year}", app.t)
    results = [(place, utc_label(now_in(place["zone"]))) for place in state["results"]]
    app.render(TILE, "flyout", views.flyout(
        home, home_facts(home), others, face=face(), seconds=seconds, lang=app.language,
        calendar_html=calendar_html,
        search_html=views.search(state["query"], results, state["search_failed"], app.t),
        t=app.t))
    state["signature"] = signature()


# --- actions -------------------------------------------------------------


def on_search(value: object) -> None:
    query = value.get("query", "") if isinstance(value, dict) else ""
    state.update(query=str(query).strip(), results=[], search_failed=False)
    if not state["query"]:
        return
    try:
        state["results"] = geocode(state["query"])
    except FETCH_ERRORS as error:
        state["search_failed"] = True
        state["results"] = zone_matches(state["query"])
        app.log("warn", "place search failed; offering time-zone names instead",
                error=str(error), hint="check network connectivity")


def index_of(value: object) -> int | None:
    try:
        index = int(str(value))
    except ValueError:
        return None
    return index if 0 <= index < len(clocks()) else None


@app.on_action(TILE)
def on_action(name: str, value: object) -> None:
    picked = clocks()
    if name == "search":
        on_search(value)
        render_all()
    elif name == "add":
        place = normalize(json.loads(value)) if isinstance(value, str) and value else None
        if place is None:
            return
        state.update(query="", results=[], search_failed=False)
        if any(c["zone"] == place["zone"] and c["label"] == place["label"] for c in picked):
            app.log("info", "clock already present", zone=place["zone"])
            render_all()
            return
        save_clocks([*picked, place])
    elif name in ("remove", "primary"):
        index = index_of(value)
        if index is None:
            return
        chosen = picked.pop(index)
        save_clocks([chosen, *picked] if name == "primary" else picked)
    elif name == "seconds":
        save({"showSeconds": not flag("showSeconds", True)})
    elif name == "face":
        save({"face": "digital" if face() == "analog" else "analog"})


@app.on_settings_changed
def on_settings_changed(_settings: dict[str, object]) -> None:
    render_all()


@app.on_ready
def on_ready() -> None:
    """First render, right after 'initialize' — the earliest moment the plugin
    locales and the user's settings exist."""
    render_all()


@app.every(60)
def on_minute() -> None:
    """Redraw only when a zone's hour or date rolled over; the time itself
    ticks in the shell, so this is a no-op most of the hour."""
    if signature() != state["signature"]:
        render_all()


if __name__ == "__main__":
    app.run()
