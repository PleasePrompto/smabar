"""Locale initialization and lookup behavior for the Plugin SDK."""

from __future__ import annotations

import io
from pathlib import Path

from _plugin_test_support import (
    init_params,
    ndjson,
    request,
    run_to_shutdown,
    sent,
    write_locale,
)

from smabar_sdk import Plugin


def test_language_defaults_to_en_and_comes_from_initialize() -> None:
    assert Plugin(reader=io.StringIO(), writer=io.StringIO()).language == "en"
    plugin, _writer = run_to_shutdown(init_params(language="de"))
    assert plugin.language == "de"


def test_invalid_language_code_falls_back_to_en(tmp_path: Path) -> None:
    plugin, _writer = run_to_shutdown(init_params(dataDir=str(tmp_path), language="../evil"))
    assert plugin.language == "en"


def test_plugin_locales_merge_en_base_with_language_overlay(tmp_path: Path) -> None:
    write_locale(tmp_path, "en", {"greet": "Hello", "cpu": "CPU"})
    write_locale(tmp_path, "de", {"greet": "Hallo"})
    plugin, _writer = run_to_shutdown(init_params(pluginDir=str(tmp_path), language="de"))
    assert plugin.t("greet") == "Hallo"  # language drop-in wins
    assert plugin.t("cpu") == "CPU"  # keys missing from the drop-in fall back to English


def test_missing_locale_files_are_not_an_error(tmp_path: Path) -> None:
    plugin, writer = run_to_shutdown(init_params(pluginDir=str(tmp_path), language="de"))
    assert plugin.t("anything") == "anything"
    assert all(m.get("method") != "log" for m in sent(writer))  # no warning for absence


def test_broken_locale_json_warns_and_plugin_keeps_running(tmp_path: Path) -> None:
    write_locale(tmp_path, "en", "{ not json")
    reader = io.StringIO(
        ndjson(
            [
                request(1, "initialize", init_params(pluginDir=str(tmp_path))),
                request(2, "ping"),
                request(3, "shutdown"),
            ]
        )
    )
    writer = io.StringIO()
    plugin = Plugin(reader=reader, writer=writer)
    plugin.run()
    warnings = [
        m for m in sent(writer) if m.get("method") == "log" and m["params"]["level"] == "warn"
    ]
    assert len(warnings) == 1
    assert "en.json" in warnings[0]["params"]["message"]
    responded_ids = {m["id"] for m in sent(writer) if "id" in m}
    assert responded_ids == {1, 2, 3}  # initialize, ping, and shutdown all answered
    assert plugin.t("x") == "x"


def test_t_prefers_plugin_locale_over_core_map_then_key(tmp_path: Path) -> None:
    write_locale(tmp_path, "en", {"hello": "Plugin hello"})
    plugin, _writer = run_to_shutdown(
        init_params(pluginDir=str(tmp_path), locale={"hello": "Hallo", "core.only": "Core"})
    )
    assert plugin.t("hello") == "Plugin hello"  # plugin locales beat the core map
    assert plugin.t("core.only") == "Core"  # core map fills what the plugin lacks
    assert plugin.t("nowhere") == "nowhere"  # last resort: the key itself
