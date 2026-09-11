"""Runtime-contract tests for the bundled crypto plugin's persisted cache."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from types import ModuleType
from typing import Protocol, cast

import pytest


class FakeApp:
    def __init__(self, data_dir: Path) -> None:
        self.data_dir = data_dir
        self.logs: list[tuple[str, str, dict[str, object]]] = []

    def log(self, level: str, message: str, **fields: object) -> None:
        self.logs.append((level, message, fields))


class CryptoModule(Protocol):
    app: FakeApp
    state: dict[str, object]

    def load_cache(self) -> bool: ...

    def priced_row(self, coin: str, entry: object, cur: str) -> dict[str, object]: ...


def load_crypto(data_dir: Path) -> CryptoModule:
    path = Path(__file__).resolve().parents[3] / "plugins" / "crypto" / "plugin.py"
    spec = importlib.util.spec_from_file_location("crypto_cache_test", path)
    assert spec is not None and spec.loader is not None
    module: ModuleType = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    loaded = cast(CryptoModule, module)
    loaded.app = FakeApp(data_dir)
    return loaded


def valid_cache() -> dict[str, object]:
    return {
        "rows": [
            {
                "id": "bitcoin",
                "known": True,
                "price": 65_000.0,
                "change": 2.5,
            }
        ],
        "currency": "eur",
        "last_ok": "12:34",
        "history": {"bitcoin": [64_000.0, 65_000.0]},
        "alert_base": {"bitcoin": 64_500.0},
    }


@pytest.mark.parametrize(
    "cached",
    [
        {
            **valid_cache(),
            "rows": [
                {
                    "id": "bitcoin",
                    "known": True,
                    "price": "not-a-number",
                    "change": 2.5,
                }
            ],
        },
        {
            **valid_cache(),
            "rows": [
                {
                    "id": "bitcoin",
                    "known": True,
                    "price": 10**400,
                    "change": 2.5,
                }
            ],
        },
        {**valid_cache(), "history": {"bitcoin": [64_000.0, "broken"]}},
        {**valid_cache(), "alert_base": {"bitcoin": True}},
        {key: value for key, value in valid_cache().items() if key != "currency"},
    ],
)
def test_invalid_nested_cache_discards_the_whole_snapshot(
    tmp_path: Path, cached: dict[str, object]
) -> None:
    module = load_crypto(tmp_path)
    previous = {
        "rows": [
            {
                "id": "previous",
                "known": True,
                "price": 1.0,
                "change": None,
            }
        ],
        "currency": "usd",
        "last_ok": "09:00",
        "history": {"previous": [1.0]},
        "alert_base": {"previous": 1.0},
    }
    module.state.update(previous)
    (tmp_path / "prices.json").write_text(json.dumps(cached), encoding="utf-8")

    assert module.load_cache() is False
    assert {key: module.state[key] for key in previous} == previous
    assert len(module.app.logs) == 1
    level, message, fields = module.app.logs[0]
    assert level == "warn"
    assert "price cache" in message
    assert "next successful" in str(fields.get("hint"))


def test_non_utf8_cache_is_discarded_with_an_actionable_warning(tmp_path: Path) -> None:
    module = load_crypto(tmp_path)
    previous_rows = module.state["rows"]
    (tmp_path / "prices.json").write_bytes(b'{"currency":"\xff"}')

    assert module.load_cache() is False
    assert module.state["rows"] is previous_rows
    assert len(module.app.logs) == 1
    level, message, fields = module.app.logs[0]
    assert level == "warn"
    assert "price cache" in message
    assert fields.get("error")
    assert "next successful" in str(fields.get("hint"))


def test_valid_cache_restores_every_persisted_field(tmp_path: Path) -> None:
    module = load_crypto(tmp_path)
    cached = valid_cache()
    (tmp_path / "prices.json").write_text(json.dumps(cached), encoding="utf-8")

    assert module.load_cache() is True
    assert {key: module.state[key] for key in cached} == cached
    assert module.app.logs == []


@pytest.mark.parametrize("invalid", [True, float("nan"), float("inf"), 10**400])
def test_live_response_rejects_non_finite_or_unrepresentable_numbers(
    tmp_path: Path, invalid: object
) -> None:
    module = load_crypto(tmp_path)

    row = module.priced_row("bitcoin", {"eur": invalid, "eur_24h_change": invalid}, "eur")

    assert row == {"id": "bitcoin", "known": True, "price": None, "change": None}
