"""Validation and loading for a plugin's own locale files."""

from __future__ import annotations

import json
from collections.abc import Callable
from pathlib import Path

type WarningReporter = Callable[[str], None]


def is_language_code(language: str) -> bool:
    """Return whether a language code is a plain file stem, never a path."""
    return language != "" and all(
        (char.isascii() and char.isalnum()) or char in "-_" for char in language
    )


def load_plugin_locales(
    plugin_dir: Path,
    language: str,
    warn: WarningReporter,
) -> dict[str, str]:
    """Merge the optional English base with the selected language overlay."""
    locales_dir = plugin_dir / "locales"
    strings = _read_locale_file(locales_dir / "en.json", warn)
    if language != "en":
        strings.update(_read_locale_file(locales_dir / f"{language}.json", warn))
    return strings


def _read_locale_file(path: Path, warn: WarningReporter) -> dict[str, str]:
    """Read one flat key-to-string JSON file; missing files are valid."""
    try:
        raw = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return {}
    except (OSError, UnicodeDecodeError) as exc:
        # UnicodeDecodeError is a ValueError, not an OSError — a locale
        # file saved as Latin-1/CP1252 must not crash-loop the plugin.
        warn(f"cannot read locale file {path}: {exc}")
        return {}
    try:
        parsed: object = json.loads(raw)
    except json.JSONDecodeError as exc:
        warn(f"ignoring broken locale file {path}: {exc}")
        return {}
    if not isinstance(parsed, dict):
        warn(f"ignoring locale file {path}: expected a flat JSON string map")
        return {}
    return {
        key: value
        for key, value in parsed.items()
        if isinstance(key, str) and isinstance(value, str)
    }
