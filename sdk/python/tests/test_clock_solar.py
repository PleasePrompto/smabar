"""Sunrise/sunset regression for the bundled clock plugin's solar module."""

import importlib.util
from datetime import date, timedelta
from pathlib import Path
from types import ModuleType
from zoneinfo import ZoneInfo


def load_solar() -> ModuleType:
    path = Path(__file__).resolve().parents[3] / "plugins/clock/solar.py"
    spec = importlib.util.spec_from_file_location("clock_solar_test", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_berlin_midsummer_matches_the_almanac() -> None:
    solar = load_solar()
    times = solar.sun_times(date(2026, 6, 21), 52.52, 13.405)
    assert times is not None
    rise, set_ = (moment.astimezone(ZoneInfo("Europe/Berlin")) for moment in times)
    # timeanddate.com lists 04:43 / 21:33 CEST for Berlin on 2026-06-21.
    assert abs(rise - rise.replace(hour=4, minute=43)) <= timedelta(minutes=5)
    assert abs(set_ - set_.replace(hour=21, minute=33)) <= timedelta(minutes=5)


def test_polar_night_has_no_sunrise() -> None:
    solar = load_solar()
    # Longyearbyen, Svalbard: the sun stays below the horizon in December.
    assert solar.sun_times(date(2026, 12, 21), 78.22, 15.63) is None
