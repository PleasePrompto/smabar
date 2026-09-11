"""Failure-atomicity and location-time tests for the bundled weather plugin."""

from __future__ import annotations

import copy
import importlib.util
import io
import json
import sys
import urllib.request
from collections.abc import Mapping
from datetime import datetime, tzinfo
from pathlib import Path
from types import ModuleType
from typing import Protocol, Self, cast

import pytest

PLUGIN_DIR = Path(__file__).resolve().parents[3] / "plugins" / "weather"
BERLIN: dict[str, object] = {
    "name": "Berlin",
    "region": "",
    "latitude": 52.52,
    "longitude": 13.405,
    "timezone": "Europe/Berlin",
}
TOKYO: dict[str, object] = {
    "name": "Tokyo",
    "region": "",
    "latitude": 35.6895,
    "longitude": 139.6917,
    "timezone": "Asia/Tokyo",
}


class FakeApp:
    def __init__(self, data_dir: Path) -> None:
        self.data_dir = data_dir
        self.language = "en"
        self.settings: dict[str, object] = {"locations": [dict(BERLIN)]}
        self.logs: list[tuple[str, str, dict[str, object]]] = []
        self.rendered: list[tuple[str, str, str]] = []

    def log(self, level: str, message: str, **fields: object) -> None:
        self.logs.append((level, message, fields))

    def render(self, tile_id: str, target: str, html: str) -> None:
        self.rendered.append((tile_id, target, html))

    def t(self, key: str) -> str:
        return key


class WeatherModule(Protocol):
    app: FakeApp
    state: dict[str, object]
    datetime: type[datetime]

    def fetch(self) -> None: ...

    def parse_hours(self, entry: Mapping[str, object]) -> list[tuple[str, float, float | None]]: ...


class FixedDateTime(datetime):
    @classmethod
    def now(cls, tz: tzinfo | None = None) -> Self:
        return cls(2026, 1, 1, 22, 30, tzinfo=tz)


def load_weather(data_dir: Path) -> WeatherModule:
    # The plugin imports its sibling views module, so its folder must be importable.
    sys.path.insert(0, str(PLUGIN_DIR))
    sys.modules.pop("views", None)
    try:
        spec = importlib.util.spec_from_file_location(
            "weather_plugin_test", PLUGIN_DIR / "plugin.py"
        )
        assert spec is not None and spec.loader is not None
        module: ModuleType = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    finally:
        sys.path.remove(str(PLUGIN_DIR))
    loaded = cast(WeatherModule, module)
    loaded.app = FakeApp(data_dir)
    return loaded


def places_of(module: WeatherModule) -> list[dict[str, object]]:
    return cast(list[dict[str, object]], module.state["places"])


def valid_payload() -> dict[str, object]:
    return {
        "utc_offset_seconds": 0,
        "timezone": "Europe/Berlin",
        "current": {
            "temperature_2m": 20.0,
            "apparent_temperature": 19.0,
            "weather_code": 1,
            "wind_speed_10m": 3.0,
            "is_day": 1,
        },
        "daily": {
            "time": ["2026-01-02"],
            "weather_code": [1],
            "temperature_2m_max": [22.0],
            "temperature_2m_min": [12.0],
            "sunrise": ["2026-01-02T08:15"],
            "sunset": ["2026-01-02T16:10"],
            "uv_index_max": [1.0],
            "precipitation_probability_max": [20],
        },
        "hourly": {
            "time": ["2026-01-02T00:00", "2026-01-02T01:00"],
            "temperature_2m": [18.0, 19.0],
            "precipitation_probability": [0, 10],
        },
    }


def previous_data() -> dict[str, object]:
    return {
        "temp": 10.0,
        "feels": 9.0,
        "wind": 4.0,
        "gusts": None,
        "humidity": None,
        "pressure": None,
        "cloud": None,
        "code": 2,
        "is_day": True,
        "timezone": "Europe/Berlin",
        "days": [
            {
                "date": "2026-01-01",
                "code": 2,
                "tmax": 12.0,
                "tmin": 5.0,
                "sunrise": "08:15",
                "sunset": "16:10",
                "uv": 1.0,
                "rain": 20.0,
            }
        ],
        "hours": [("09", 9.0, 0.0)],
    }


def replace_payload_value(
    payload: dict[str, object], path: tuple[str, ...], value: object
) -> object:
    if not path:
        return value
    parent = payload
    for key in path[:-1]:
        child = parent[key]
        assert isinstance(child, dict)
        parent = child
    parent[path[-1]] = value
    return payload


def serve(monkeypatch: pytest.MonkeyPatch, payload: object) -> None:
    def response(_url: str, *, timeout: int) -> io.StringIO:
        assert timeout == 10
        return io.StringIO(json.dumps(payload))

    monkeypatch.setattr(urllib.request, "urlopen", response)


@pytest.mark.parametrize(
    ("path", "invalid"),
    [
        pytest.param((), [], id="payload-is-not-an-object"),
        pytest.param(("current",), [], id="current-is-not-an-object"),
        pytest.param(("daily",), [], id="daily-is-not-an-object"),
        pytest.param(("hourly",), [], id="hourly-is-not-an-object"),
        pytest.param(("daily", "time"), "2026-01-02", id="daily-value-is-not-an-array"),
        pytest.param(("hourly", "temperature_2m"), {}, id="hourly-value-is-not-an-array"),
        pytest.param(("current", "temperature_2m"), True, id="boolean-number"),
        pytest.param(("daily", "temperature_2m_min"), [None], id="null-daily-number"),
        pytest.param(("daily", "temperature_2m_max"), [float("nan")], id="non-finite-daily-number"),
        pytest.param(("current", "temperature_2m"), 10**1000, id="overflowing-number"),
        pytest.param(
            ("hourly", "temperature_2m"),
            [18.0, float("inf")],
            id="non-finite-hourly-number",
        ),
        pytest.param(("utc_offset_seconds",), True, id="boolean-utc-offset"),
        pytest.param(("utc_offset_seconds",), 1.5, id="fractional-utc-offset"),
        pytest.param(("daily", "weather_code"), [], id="unequal-daily-arrays"),
        pytest.param(("hourly", "time"), [], id="unequal-hourly-arrays"),
        pytest.param(("timezone",), 7, id="timezone-is-not-a-string"),
    ],
)
def test_invalid_api_shapes_keep_the_entire_previous_successful_snapshot(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    path: tuple[str, ...],
    invalid: object,
) -> None:
    module = load_weather(tmp_path)
    previous = previous_data()
    module.state["places"] = [{**BERLIN, "data": copy.deepcopy(previous)}]
    module.state["last_ok"] = "09:00"
    serve(monkeypatch, replace_payload_value(valid_payload(), path, invalid))

    module.fetch()

    assert places_of(module)[0]["data"] == previous
    assert module.state["last_ok"] == "09:00"
    assert module.state["error"]
    assert [level for level, _message, _fields in module.app.logs] == ["warn"]
    assert not (tmp_path / "forecast.json").exists()


def test_successful_fetch_parses_every_place_and_caches_the_forecast(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    module = load_weather(tmp_path)
    module.app.settings["locations"] = [dict(BERLIN), dict(TOKYO)]
    tokyo = valid_payload()
    tokyo["timezone"] = "Asia/Tokyo"
    # Two or more places come back as an array, one place as a bare object.
    serve(monkeypatch, [valid_payload(), tokyo])

    module.fetch()

    places = places_of(module)
    assert [place["name"] for place in places] == ["Berlin", "Tokyo"]
    berlin = cast(dict[str, object], places[0]["data"])
    assert berlin["temp"] == 20.0
    assert berlin["days"] == [
        {
            "date": "2026-01-02",
            "code": 1,
            "tmax": 22.0,
            "tmin": 12.0,
            "sunrise": "08:15",
            "sunset": "16:10",
            "uv": 1.0,
            "rain": 20.0,
        }
    ]
    assert module.state["error"] == ""
    cached = json.loads((tmp_path / "forecast.json").read_text(encoding="utf-8"))
    assert [place["name"] for place in cached["places"]] == ["Berlin", "Tokyo"]
    assert [level for level, _message, _fields in module.app.logs] == ["info"]


def test_hour_window_uses_the_location_utc_offset(tmp_path: Path) -> None:
    module = load_weather(tmp_path)
    module.datetime = FixedDateTime
    payload: dict[str, object] = {
        "utc_offset_seconds": 3 * 60 * 60,
        "hourly": {
            "time": [
                "2026-01-02T00:00",
                "2026-01-02T01:00",
                "2026-01-02T02:00",
            ],
            "temperature_2m": [10.0, 11.0, 12.0],
            "precipitation_probability": [20, 30, None],
        },
    }

    assert module.parse_hours(payload) == [("01", 11.0, 30.0), ("02", 12.0, None)]
