"""Named Clock zones work with only the declared tzdata package, as on Windows."""

import importlib.util
import sys
import tomllib
import zoneinfo
from datetime import UTC, datetime
from pathlib import Path

import pytest


def test_clock_named_zones_without_system_database(monkeypatch: pytest.MonkeyPatch) -> None:
    path = Path(__file__).resolve().parents[3] / "plugins/clock/plugin.py"
    metadata = path.read_text(encoding="utf-8").split("# /// script\n", 1)[1].split("# ///", 1)[0]
    script = tomllib.loads("\n".join(line.removeprefix("# ") for line in metadata.splitlines()))
    assert any(dependency.split("==")[0] == "tzdata" for dependency in script["dependencies"])

    monkeypatch.syspath_prepend(str(path.parent))
    monkeypatch.delitem(sys.modules, "views", raising=False)
    spec = importlib.util.spec_from_file_location("clock_zones_test", path)
    assert spec is not None and spec.loader is not None
    clock = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(clock)

    previous_path = zoneinfo.TZPATH
    zoneinfo.reset_tzpath(())
    zoneinfo.ZoneInfo.clear_cache()
    try:
        for label, zone in (
            ("Berlin", "Europe/Berlin"),
            ("New York", "America/New_York"),
            ("Tokyo", "Asia/Tokyo"),
        ):
            assert clock.normalize({"label": label, "zone": zone}) is not None
            assert any(place["zone"] == zone for place in clock.zone_matches(label))

        for zone, instant, expected in (
            ("Europe/Berlin", "2026-03-29T00:30:00+00:00", "UTC+1"),
            ("Europe/Berlin", "2026-03-29T01:30:00+00:00", "UTC+2"),
            ("Europe/Berlin", "2026-10-25T01:30:00+00:00", "UTC+1"),
            ("America/New_York", "2026-03-08T06:30:00+00:00", "UTC\N{MINUS SIGN}5"),
            ("America/New_York", "2026-03-08T07:30:00+00:00", "UTC\N{MINUS SIGN}4"),
            ("America/New_York", "2026-11-01T06:30:00+00:00", "UTC\N{MINUS SIGN}5"),
            ("Asia/Tokyo", "2026-03-29T01:30:00+00:00", "UTC+9"),
            ("Asia/Tokyo", "2026-10-25T01:30:00+00:00", "UTC+9"),
        ):
            local = datetime.fromisoformat(instant).astimezone(zoneinfo.ZoneInfo(zone))
            assert clock.utc_label(local) == expected

        now = datetime(2026, 3, 1, tzinfo=UTC)
        assert clock.next_transition("Europe/Berlin", now) == datetime(2026, 3, 29, 1, tzinfo=UTC)
        assert clock.next_transition("America/New_York", now) == datetime(2026, 3, 8, 7, tzinfo=UTC)
        assert clock.next_transition("Asia/Tokyo", now) is None
    finally:
        zoneinfo.reset_tzpath(previous_path)
        zoneinfo.ZoneInfo.clear_cache()
