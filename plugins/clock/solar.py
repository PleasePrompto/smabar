"""Sunrise and sunset from the NOAA "sunrise equation" — stdlib only, offline.

Accuracy is a few minutes, which is all a clock flyout needs. Polar day and
polar night (the sun never crosses the horizon) return None.
"""

import math
from datetime import date, datetime, timedelta, timezone

J2000 = datetime(2000, 1, 1, 12, tzinfo=timezone.utc)
OBLIQUITY = math.radians(23.4397)
# Refraction plus the solar disc radius: the "official" horizon.
HORIZON = math.radians(-0.833)


def sun_times(day: date, latitude: float, longitude: float) -> tuple[datetime, datetime] | None:
    """UTC sunrise and sunset for `day` at an east-positive longitude."""
    days = (day - J2000.date()).days
    solar_noon = days + 0.0009 - longitude / 360
    mean_anomaly = math.radians((357.5291 + 0.98560028 * solar_noon) % 360)
    centre = (
        1.9148 * math.sin(mean_anomaly)
        + 0.02 * math.sin(2 * mean_anomaly)
        + 0.0003 * math.sin(3 * mean_anomaly)
    )
    ecliptic = math.radians((math.degrees(mean_anomaly) + centre + 180 + 102.9372) % 360)
    transit = solar_noon + 0.0053 * math.sin(mean_anomaly) - 0.0069 * math.sin(2 * ecliptic)
    declination = math.asin(math.sin(ecliptic) * math.sin(OBLIQUITY))
    lat = math.radians(latitude)
    cos_hour = (math.sin(HORIZON) - math.sin(lat) * math.sin(declination)) / (
        math.cos(lat) * math.cos(declination)
    )
    if not -1 <= cos_hour <= 1:
        return None
    hour_angle = math.degrees(math.acos(cos_hour)) / 360
    rise = J2000 + timedelta(days=transit - hour_angle)
    set_ = J2000 + timedelta(days=transit + hour_angle)
    return rise, set_
