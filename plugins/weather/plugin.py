# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Weather tile: current conditions and a 7-day forecast for several places.

Endpoints (Open-Meteo, keyless; shapes verified live 2026-09-05):
- https://api.open-meteo.com/v1/forecast takes comma-separated latitude and
  longitude lists, so ONE request serves every configured place. The JSON is
  an array for two or more places and a bare object for one. "current" is a
  flat object, "hourly" and "daily" hold parallel arrays; every stamp is local
  to its place (timezone=auto).
- https://geocoding-api.open-meteo.com/v1/search turns a city or country name
  into coordinates, time zone and country, in the user's language.
The free tier allows ~10,000 calls/day; polling every 15 minutes (~96 calls)
stays far below that.

The last forecast is cached in app.data_dir, so a restart shows yesterday's
numbers with a quiet "updated …" line instead of an empty tile.
"""

import json
import math
import os
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Mapping
from datetime import UTC, datetime, timedelta

from smabar_sdk import Plugin
import views

app = Plugin()
TILE = "weather"
API = "https://api.open-meteo.com/v1/forecast"
GEOCODE_API = "https://geocoding-api.open-meteo.com/v1/search"
POLL_SECONDS = 900.0  # 15 min — conservative for the free Open-Meteo tier
FORECAST_DAYS = 7
SPARK_HOURS = 24
MAX_RESULTS = 6
CACHE_FILE = "forecast.json"
DEFAULT_PLACES = [{"name": "Berlin", "region": "", "latitude": 52.52,
                   "longitude": 13.405, "timezone": "Europe/Berlin"}]
UNIT_PARAMS = {"metric": {}, "imperial": {"temperature_unit": "fahrenheit", "wind_speed_unit": "mph"}}
FETCH_ERRORS = (urllib.error.URLError, TimeoutError, ValueError, TypeError)

# places: configured locations, each with the last parsed forecast under "data"
# (None until the first successful fetch). "applied" mirrors the settings this
# plugin fetched for, so the settings.changed echo of its own set_settings does
# not trigger a second request.
state: dict = {"places": [], "selected": 0, "error": "", "last_ok": "", "applied": None,
               "query": "", "results": [], "search_failed": False}


# --- validation ----------------------------------------------------------


def expect_mapping(value: object, field: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise TypeError(f"{field} must be an object")
    return value


def expect_list(value: object, field: str) -> list[object]:
    if not isinstance(value, list):
        raise TypeError(f"{field} must be an array")
    return value


def maybe_number(value: object) -> float | None:
    """A finite number, or None for Open-Meteo's null gaps."""
    if value is None or isinstance(value, bool) or not isinstance(value, int | float):
        return None
    try:
        number = float(value)
    except OverflowError:
        return None
    return number if math.isfinite(number) else None


def expect_number(value: object, field: str) -> float:
    number = maybe_number(value)
    if number is None:
        raise TypeError(f"{field} must be a finite number")
    return number


def expect_integer(value: object, field: str) -> int:
    number = expect_number(value, field)
    if not number.is_integer():
        raise ValueError(f"{field} must be an integer")
    return int(number)


def expect_text(value: object, field: str) -> str:
    if not isinstance(value, str):
        raise TypeError(f"{field} must be a string")
    return value


# --- settings ------------------------------------------------------------


def normalize(raw: object) -> dict | None:
    """One place from settings or a search result; None when unusable."""
    if not isinstance(raw, dict):
        return None
    name, zone = raw.get("name"), raw.get("timezone")
    lat, lon = maybe_number(raw.get("latitude")), maybe_number(raw.get("longitude"))
    if not isinstance(name, str) or not name.strip() or lat is None or lon is None:
        app.log("warn", "ignoring a place without name or coordinates", place=repr(raw)[:120],
                hint="search the place again in the flyout")
        return None
    return {"name": name.strip(), "region": str(raw.get("region") or ""), "latitude": lat,
            "longitude": lon, "timezone": zone if isinstance(zone, str) else ""}


def locations() -> list[dict]:
    """Configured places; the first one owns the tile. A missing key means the
    documented Berlin default, an explicit empty list means no places."""
    raw = app.settings.get("locations")
    if raw is None:
        return [dict(place) for place in DEFAULT_PLACES]
    if not isinstance(raw, list):
        app.log("warn", 'settings key "locations" is not a list; using the Berlin default')
        return [dict(place) for place in DEFAULT_PLACES]
    return [place for place in map(normalize, raw) if place is not None]


def units() -> str:
    return "imperial" if app.settings.get("units") == "imperial" else "metric"


def flag(name: str, default: bool) -> bool:
    value = app.settings.get(name)
    return value if isinstance(value, bool) else default


def applied_signature(settings: Mapping[str, object]) -> str:
    return json.dumps({"locations": settings.get("locations"), "units": settings.get("units")}, sort_keys=True)


def save_locations(places: list[dict]) -> None:
    """Persist and refetch; settings.set REPLACES the whole object."""
    app.set_settings({**app.settings, "locations": places})
    state["applied"] = applied_signature(app.settings)
    fetch()


def place_key(place: Mapping[str, object]) -> str:
    return f"{place['latitude']:.4f},{place['longitude']:.4f}"


# --- cache ---------------------------------------------------------------


def load_cache() -> dict[str, dict]:
    """Forecast data by place key from the last successful fetch, if any."""
    path = app.data_dir / CACHE_FILE
    try:
        cached = json.loads(path.read_text(encoding="utf-8"))
        places = expect_list(expect_mapping(cached, "cache").get("places"), "cache.places")
        state["last_ok"] = expect_text(cached.get("last_ok", ""), "cache.last_ok")
        return {place_key(place): place["data"] for place in places
                if isinstance(place, dict) and isinstance(place.get("data"), dict)
                and maybe_number(place.get("latitude")) is not None
                and maybe_number(place.get("longitude")) is not None}
    except FileNotFoundError:
        return {}
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, TypeError, ValueError, KeyError) as error:
        app.log("warn", "ignoring an invalid forecast cache; starting without cached data",
                error=f"{type(error).__name__}: {error}",
                hint=f"remove {path.name}; the next successful update replaces it")
        return {}


def save_cache() -> None:
    """Write + rename: a plugin killed mid-write must not leave half a file."""
    path = app.data_dir / CACHE_FILE
    temporary = path.with_suffix(".tmp")
    try:
        temporary.write_text(json.dumps({"places": state["places"], "last_ok": state["last_ok"]}),
                             encoding="utf-8")
        os.replace(temporary, path)
    except OSError as error:
        app.log("warn", "could not cache the forecast", error=str(error))


# --- fetching ------------------------------------------------------------


def parse_hours(entry: Mapping[str, object]) -> list[tuple[str, float, float | None]]:
    """(hour label, temperature, rain probability) for the next 24 local hours."""
    hourly = expect_mapping(entry.get("hourly"), "hourly")
    times = expect_list(hourly.get("time"), "hourly.time")
    temps = expect_list(hourly.get("temperature_2m"), "hourly.temperature_2m")
    rain = expect_list(hourly.get("precipitation_probability"), "hourly.precipitation_probability")
    values = [(expect_text(stamp, f"hourly.time[{i}]"), expect_number(temp, f"hourly.temperature_2m[{i}]"),
               maybe_number(chance))
              for i, (stamp, temp, chance) in enumerate(zip(times, temps, rain, strict=True))]
    # The stamps are local to the PLACE (timezone=auto); the machine clock would
    # skew the window start for any far-away place.
    offset_seconds = expect_integer(entry.get("utc_offset_seconds"), "utc_offset_seconds")
    if not -86400 <= offset_seconds <= 86400:
        raise ValueError("utc_offset_seconds is outside the supported one-day range")
    now_iso = (datetime.now(UTC) + timedelta(seconds=offset_seconds)).strftime("%Y-%m-%dT%H:00")
    start = next((i for i, (stamp, _t, _r) in enumerate(values) if stamp >= now_iso), 0)
    return [(stamp[11:13], temp, chance) for stamp, temp, chance in values[start:start + SPARK_HOURS]]


def parse_days(entry: Mapping[str, object]) -> list[dict]:
    daily = expect_mapping(entry.get("daily"), "daily")
    columns = {key: expect_list(daily.get(key), f"daily.{key}") for key in
               ("time", "weather_code", "temperature_2m_max", "temperature_2m_min", "sunrise", "sunset",
                "uv_index_max", "precipitation_probability_max")}
    days = []
    for i, row in enumerate(zip(*columns.values(), strict=True)):
        stamp, code, tmax, tmin, rise, set_, uv, rain = row
        days.append({"date": expect_text(stamp, f"daily.time[{i}]"),
                     "code": expect_integer(code, f"daily.weather_code[{i}]"),
                     "tmax": expect_number(tmax, f"daily.temperature_2m_max[{i}]"),
                     "tmin": expect_number(tmin, f"daily.temperature_2m_min[{i}]"),
                     "sunrise": expect_text(rise, f"daily.sunrise[{i}]")[11:16],
                     "sunset": expect_text(set_, f"daily.sunset[{i}]")[11:16],
                     "uv": maybe_number(uv), "rain": maybe_number(rain)})
    return days


def parse_place(entry: Mapping[str, object]) -> dict:
    current = expect_mapping(entry.get("current"), "current")
    return {
        "temp": expect_number(current.get("temperature_2m"), "current.temperature_2m"),
        "feels": expect_number(current.get("apparent_temperature"), "current.apparent_temperature"),
        "wind": expect_number(current.get("wind_speed_10m"), "current.wind_speed_10m"),
        "gusts": maybe_number(current.get("wind_gusts_10m")),
        "humidity": maybe_number(current.get("relative_humidity_2m")),
        "pressure": maybe_number(current.get("surface_pressure")),
        "cloud": maybe_number(current.get("cloud_cover")),
        "code": expect_integer(current.get("weather_code"), "current.weather_code"),
        "is_day": expect_integer(current.get("is_day"), "current.is_day") == 1,
        "timezone": expect_text(entry.get("timezone"), "timezone"),
        "days": parse_days(entry),
        "hours": parse_hours(entry),
    }


def fetch() -> None:
    """One request for every place; on failure the last known data stays."""
    places = locations()
    if not places:
        state.update(places=[], error="")
        render_all()
        return
    query = urllib.parse.urlencode({
        "latitude": ",".join(f"{p['latitude']:.4f}" for p in places),
        "longitude": ",".join(f"{p['longitude']:.4f}" for p in places),
        "current": "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,"
                   "wind_speed_10m,wind_gusts_10m,surface_pressure,cloud_cover,is_day",
        "hourly": "temperature_2m,precipitation_probability",
        "daily": "weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,"
                 "uv_index_max,precipitation_probability_max",
        "forecast_days": FORECAST_DAYS,
        "timezone": "auto",
        **UNIT_PARAMS[units()],
    })
    try:
        with urllib.request.urlopen(f"{API}?{query}", timeout=10) as response:
            payload = json.load(response)
        entries = payload if isinstance(payload, list) else [payload]
        if len(entries) != len(places):
            raise ValueError(f"expected {len(places)} forecasts, got {len(entries)}")
        fresh = [{**place, "timezone": place["timezone"] or data["timezone"], "data": data}
                 for place, data in zip(places, (parse_place(expect_mapping(e, "forecast")) for e in entries),
                                        strict=True)]
    except FETCH_ERRORS as error:
        known = {place_key(p): p["data"] for p in state["places"]}
        state["places"] = [{**place, "data": known.get(place_key(place))} for place in places]
        state["error"] = str(error) or type(error).__name__
        app.log("warn", "weather fetch failed; showing the last known data", error=str(error),
                hint="check network connectivity; places are edited in the flyout")
    else:
        state.update(places=fresh, error="", last_ok=datetime.now(UTC).astimezone().strftime("%H:%M"))
        save_cache()
        app.log("info", "weather updated", places=",".join(p["name"] for p in fresh))
    render_all()


def geocode(query: str) -> list[dict]:
    params = urllib.parse.urlencode({"name": query, "count": MAX_RESULTS, "language": app.language,
                                     "format": "json"})
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
        place = normalize({"name": name, "region": region, "latitude": item.get("latitude"),
                           "longitude": item.get("longitude"), "timezone": item.get("timezone")})
        if place is not None:
            places.append(place)
    return places


# --- rendering -----------------------------------------------------------


def render_all() -> None:
    places = state["places"]
    state["selected"] = min(state["selected"], max(len(places) - 1, 0))
    layout = str(app.settings.get("coverLayout", "segmentHeader"))
    app.render(TILE, "tile", views.tile(places, layout, flag("tileRotates", False), state["error"], app.t))
    app.render(TILE, "hover", views.hover(places, app.language, app.t))
    app.render(TILE, "flyout", views.flyout(
        places, state["selected"], units=units(), lang=app.language, error=state["error"],
        last_ok=state["last_ok"],
        search_html=views.search(state["query"], state["results"], state["search_failed"], app.t),
        t=app.t))


# --- lifecycle and actions -----------------------------------------------


@app.on_ready
def show_cached_forecast() -> None:
    """Render the cached forecast BEFORE the first request goes out, so the bar
    is never blank while the network is slow — or down."""
    cached = load_cache()
    state["places"] = [{**place, "data": cached.get(place_key(place))} for place in locations()]
    state["applied"] = applied_signature(app.settings)
    if cached:
        app.log("info", "restored the cached forecast", places=len(cached))
    render_all()


@app.every(POLL_SECONDS)
def poll() -> None:
    fetch()


def index_of(value: object) -> int | None:
    try:
        index = int(str(value))
    except ValueError:
        return None
    return index if 0 <= index < len(state["places"]) else None


@app.on_action(TILE)
def on_action(name: str, value: object) -> None:
    if name == "refresh":
        fetch()
    elif name == "select":
        index = index_of(value)
        if index is not None:
            state["selected"] = index
            render_all()
    elif name == "search":
        query = value.get("query", "") if isinstance(value, dict) else ""
        state.update(query=str(query).strip(), results=[], search_failed=False)
        if state["query"]:
            try:
                state["results"] = geocode(state["query"])
            except FETCH_ERRORS as error:
                state["search_failed"] = True
                app.log("warn", "place search failed", error=str(error), hint="check network connectivity")
        render_all()
    elif name == "add":
        place = normalize(json.loads(value)) if isinstance(value, str) and value else None
        if place is None:
            return
        state.update(query="", results=[], search_failed=False)
        current = locations()
        if any(place_key(p) == place_key(place) for p in current):
            app.log("info", "place already present", place=place["name"])
            render_all()
            return
        save_locations([*current, place])
    elif name in ("remove", "primary"):
        index = index_of(value)
        if index is None:
            return
        current = locations()
        chosen = current.pop(index)
        state["selected"] = 0
        save_locations([chosen, *current] if name == "primary" else current)


@app.on_settings_changed
def settings_changed(settings: dict) -> None:
    signature = applied_signature(settings)
    if signature == state["applied"]:
        render_all()  # cover layout, rotation: a redraw is enough
        return
    state["applied"] = signature
    fetch()


if __name__ == "__main__":
    app.run()
