#!/usr/bin/env python3
"""Refresh or verify smabar's checked-in Google Fonts catalog.

Normal mode is maintainer-only and uses Google's documented Developer API.
`--check` is intentionally offline: CI verifies that the checked-in artifact is
complete, internally consistent, and in the canonical generated format. The
explicit bootstrap flag exists only for the initial keyless catalog import.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates/smabar-core/src/fonts/google-fonts.json"
DEVELOPER_API = (
    "https://www.googleapis.com/webfonts/v1/webfonts"
    "?sort=popularity&capability=WOFF2&capability=VF"
)
BOOTSTRAP_SOURCE = "https://fonts.google.com/metadata/fonts"
ALLOWED_SOURCES = {DEVELOPER_API, BOOTSTRAP_SOURCE}
MIN_FAMILY_COUNT = 1_900
CATEGORIES = {
    "Sans Serif": "sans-serif",
    "Serif": "serif",
    "Monospace": "monospace",
    "Display": "display",
    "Handwriting": "handwriting",
}
API_CATEGORIES = set(CATEGORIES.values())


def font_id(family: str) -> str:
    return re.sub(r"-+", "-", re.sub(r"[^a-z0-9]+", "-", family.lower())).strip("-")


def variant_key(variant: str) -> tuple[int, bool]:
    italic = variant.endswith("i")
    weight = variant[:-1] if italic else variant
    return int(weight), italic


def fetch_json(url: str, headers: dict[str, str]) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "smabar-google-font-catalog-generator/1", **headers},
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        raw = response.read().decode("utf-8")
    if raw.startswith(")]}'"):
        raw = raw.split("\n", 1)[-1]
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("Google Fonts metadata root is not an object")
    return value


def bootstrap_rows(metadata: dict[str, Any]) -> list[dict[str, Any]]:
    source_rows = metadata.get("familyMetadataList")
    if not isinstance(source_rows, list):
        raise ValueError("Google Fonts metadata has no familyMetadataList")
    rows: list[dict[str, Any]] = []
    for row in source_rows:
        if not isinstance(row, dict) or not row.get("isOpenSource"):
            raise ValueError("catalog contains a non-open-source or malformed family")
        family = row.get("family")
        category = CATEGORIES.get(row.get("category"))
        fonts = row.get("fonts")
        if not isinstance(family, str) or category is None or not isinstance(fonts, dict):
            raise ValueError(f"malformed family metadata: {family!r}")
        variants = sorted(
            (key for key in fonts if re.fullmatch(r"\d{1,4}i?", key)),
            key=variant_key,
        )
        if not variants:
            raise ValueError(f"{family} has no usable variants")
        rows.append(
            {
                "family": family,
                "category": category,
                "variants": variants,
                "popularity": int(row.get("popularity", 0)),
            }
        )
    return rows


def developer_rows(response: dict[str, Any]) -> list[dict[str, Any]]:
    source_rows = response.get("items")
    if not isinstance(source_rows, list):
        raise ValueError("Google Fonts Developer API response has no items")
    rows: list[dict[str, Any]] = []
    for rank, row in enumerate(source_rows, start=1):
        family = row.get("family") if isinstance(row, dict) else None
        category = row.get("category") if isinstance(row, dict) else None
        variants = row.get("variants") if isinstance(row, dict) else None
        if not isinstance(family, str) or category not in API_CATEGORIES:
            raise ValueError(f"malformed Developer API family: {family!r}")
        if not isinstance(variants, list):
            raise ValueError(f"{family} has no variants")
        normalized = []
        for variant in variants:
            if variant == "regular":
                variant = "400"
            elif variant == "italic":
                variant = "400i"
            elif isinstance(variant, str) and variant.endswith("italic"):
                variant = f"{variant.removesuffix('italic')}i"
            if isinstance(variant, str) and re.fullmatch(r"\d{1,4}i?", variant):
                normalized.append(variant)
        if not normalized:
            raise ValueError(f"{family} has no usable variants")
        rows.append(
            {
                "family": family,
                "category": category,
                "variants": sorted(set(normalized), key=variant_key),
                "popularity": rank,
            }
        )
    return rows


def build_catalog(rows: list[dict[str, Any]], source: str) -> dict[str, Any]:
    families: list[dict[str, Any]] = []
    for row in rows:
        family = row["family"]
        identifier = font_id(family)
        category = row["category"]
        families.append(
            {
                "id": identifier,
                "family": family,
                "category": category,
                "monospaced": category == "monospace",
                "variants": row["variants"],
                "popularity": row["popularity"],
            }
        )

    families.sort(key=lambda item: (item["family"].casefold(), item["family"]))
    catalog = {
        "version": 1,
        "source": source,
        "familyCount": len(families),
        "families": families,
    }
    validate_catalog(catalog)
    return catalog


def validate_catalog(catalog: dict[str, Any]) -> None:
    root_keys = {"version", "source", "familyCount", "families"}
    if set(catalog) != root_keys:
        raise ValueError(f"catalog keys must be exactly {sorted(root_keys)}")
    if catalog.get("version") != 1 or catalog.get("source") not in ALLOWED_SOURCES:
        raise ValueError("unexpected catalog version or provenance")
    families = catalog.get("families")
    if not isinstance(families, list) or len(families) < MIN_FAMILY_COUNT:
        raise ValueError(f"catalog must contain at least {MIN_FAMILY_COUNT} families")
    if catalog.get("familyCount") != len(families):
        raise ValueError("familyCount does not match families")
    ids = [item.get("id") for item in families if isinstance(item, dict)]
    names = [item.get("family") for item in families if isinstance(item, dict)]
    if len(ids) != len(families) or len(set(ids)) != len(ids):
        raise ValueError("catalog IDs are missing or duplicated")
    if len(set(names)) != len(names):
        raise ValueError("catalog family names are duplicated")
    family_keys = {"id", "family", "category", "monospaced", "variants", "popularity"}
    for item in families:
        if not isinstance(item, dict) or set(item) != family_keys:
            raise ValueError(f"font keys must be exactly {sorted(family_keys)}")
        if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", item["id"]):
            raise ValueError(f"unsafe catalog ID: {item['id']!r}")
        if item.get("category") not in API_CATEGORIES:
            raise ValueError(f"unknown category for {item['family']}")
        variants = item.get("variants")
        if not isinstance(variants, list) or not variants:
            raise ValueError(f"missing variants for {item['family']}")
        if not isinstance(item.get("popularity"), int) or item["popularity"] < 0:
            raise ValueError(f"invalid popularity for {item['family']}")


def rendered(catalog: dict[str, Any]) -> str:
    return json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify the checked-in file offline")
    parser.add_argument(
        "--bootstrap-metadata",
        action="store_true",
        help="initial keyless import only; normal updates use the Developer API",
    )
    args = parser.parse_args()

    if args.check:
        try:
            current = OUTPUT.read_text(encoding="utf-8")
            catalog = json.loads(current)
            validate_catalog(catalog)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"{OUTPUT}: {error}", file=sys.stderr)
            return 1
        if current != rendered(catalog):
            print(f"{OUTPUT} is not in canonical generated form", file=sys.stderr)
            return 1
        print(f"Google Fonts catalog OK ({catalog['familyCount']} families)")
        return 0

    try:
        if args.bootstrap_metadata:
            metadata = fetch_json(BOOTSTRAP_SOURCE, {})
            catalog = build_catalog(bootstrap_rows(metadata), BOOTSTRAP_SOURCE)
        else:
            api_key = os.environ.get("GOOGLE_FONTS_API_KEY")
            if not api_key:
                print("GOOGLE_FONTS_API_KEY is required to refresh the catalog", file=sys.stderr)
                return 2
            response = fetch_json(DEVELOPER_API, {"X-Goog-Api-Key": api_key})
            catalog = build_catalog(developer_rows(response), DEVELOPER_API)
    except (OSError, ValueError, urllib.error.URLError) as error:
        # The key is sent only as a header and is never included in this error.
        print(f"Google Fonts catalog refresh failed: {error}", file=sys.stderr)
        return 1
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(rendered(catalog), encoding="utf-8")
    print(f"wrote {OUTPUT} ({catalog['familyCount']} families)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
