//! Behavior tests for the theme MCP tools (theme_list, theme_get,
//! theme_write) plus theme activation via settings_set.

use std::fs;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};

use crate::themes;

use super::tests::{test_handler, unwrap_json};
use super::types::{SettingsSetParams, ThemeGetParams, ThemeWriteParams};

fn write_params(name: &str, tokens: Value) -> Parameters<ThemeWriteParams> {
    Parameters(ThemeWriteParams {
        name: name.to_string(),
        tokens,
        settings: None,
        meta: None,
    })
}

fn write_params_with_settings(
    name: &str,
    tokens: Value,
    settings: Value,
) -> Parameters<ThemeWriteParams> {
    Parameters(ThemeWriteParams {
        name: name.to_string(),
        tokens,
        settings: Some(settings),
        meta: None,
    })
}

fn get_params(name: Option<&str>) -> Parameters<ThemeGetParams> {
    Parameters(ThemeGetParams {
        name: name.map(str::to_string),
        contract_path: None,
        offset: None,
    })
}

#[tokio::test]
async fn theme_list_marks_the_bundled_default_as_active() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.theme_list().await).expect("list themes");
    // The four compiled-in full themes, no drop-ins yet.
    assert_eq!(result.themes.len(), 4);
    for theme in &result.themes {
        assert_eq!(theme.source, "bundled", "{}", theme.name);
        // Preview colors are always extracted.
        assert!(!theme.colors.accent.is_empty());
    }
    let default = result
        .themes
        .iter()
        .find(|t| t.name == "default")
        .expect("default listed");
    assert!(default.active);
    assert_eq!(
        Some(&default.colors.accent),
        themes::bundled_default().get("--sb-accent")
    );
}

#[tokio::test]
async fn theme_contract_and_tool_descriptions_publish_the_font_workflow() {
    let contract = themes::contract::export();
    let slots = &contract["fonts"]["slots"];
    for (slot, family, source) in [
        ("sans", "--sb-font-sans", "--sb-font-sans-source"),
        ("mono", "--sb-font-mono", "--sb-font-mono-source"),
    ] {
        assert_eq!(slots[slot]["familyToken"], family);
        assert_eq!(slots[slot]["sourceToken"], source);
        assert_eq!(contract["baseTokens"][family]["type"], "font-family");
        assert_eq!(contract["baseTokens"][source]["type"], "font-source");
    }

    let google = &contract["fonts"]["sources"]["google"];
    assert_eq!(google["tokenValue"], "google:<catalog-id>");
    assert_eq!(google["discovery"]["tool"], "font_list");
    assert_eq!(google["discovery"]["idField"], "id");
    assert_eq!(google["discovery"]["familyValueField"], "cssStack");
    assert_eq!(google["downloadTrigger"], "theme activation");
    assert_eq!(google["installationScope"], "smabar-only");
    assert_eq!(google["osInstalled"], false);
    for field in ["fallback", "canonicalization", "rejected"] {
        assert!(
            !google[field].is_null(),
            "contract.fonts.google misses {field}"
        );
    }
    assert_eq!(
        contract["fonts"]["sources"]["system"]["discovery"]["tool"],
        "font_list"
    );

    let (_dir, mcp) = test_handler().await;
    unwrap_json(
        mcp.theme_write(write_params(
            "font-roundtrip",
            json!({
                "--sb-font-sans": "Wrong Family, sans-serif",
                "--sb-font-sans-source": "google:roboto"
            }),
        ))
        .await,
    )
    .expect("write Google font theme");
    let resolved = unwrap_json(mcp.theme_get(get_params(Some("font-roundtrip"))).await)
        .expect("resolve Google font theme");
    assert!(resolved.tokens["--sb-font-sans"].starts_with("'Roboto',"));

    let tools = super::SmabarMcp::theme_tool_router().list_all();
    let description = |name: &str| {
        tools
            .iter()
            .find(|tool| tool.name == name)
            .and_then(|tool| tool.description.clone())
            .unwrap_or_else(|| panic!("tool {name} has no description"))
    };
    let get = description("theme_get");
    for detail in ["CALL THIS FIRST", "referenceThemes", "font"] {
        assert!(
            get.contains(detail),
            "theme_get description misses {detail}"
        );
    }
    let write = description("theme_write");
    for detail in [
        "EXACT WORKFLOW",
        "font_list",
        "google:<catalog-id>",
        "settings_set",
        "theme_get(name)",
    ] {
        assert!(
            write.contains(detail),
            "theme_write description misses {detail}"
        );
    }
}

#[tokio::test]
async fn theme_write_then_list_get_and_activate_roundtrip() {
    let (_dir, mcp) = test_handler().await;

    let ack = unwrap_json(
        mcp.theme_write(write_params(
            "neon",
            json!({
                "--sb-accent": "#00ff88",
                "--sb-chart-1": "#00aaff",
                "--sb-scale": "1.2",
                "--my-plugin-glow": "0 0 8px #00ff88"
            }),
        ))
        .await,
    )
    .expect("write theme");
    assert!(ack.message.contains("neon.json"));
    assert!(ack.message.contains("settings_set"));

    // The file on disk is valid flat JSON.
    let on_disk = fs::read_to_string(mcp.paths.themes_dir().join("neon.json")).expect("read");
    let parsed: themes::ThemeMap = serde_json::from_str(&on_disk).expect("parse");
    assert_eq!(parsed.len(), 4);

    let list = unwrap_json(mcp.theme_list().await).expect("list themes");
    let neon = list
        .themes
        .iter()
        .find(|t| t.name == "neon")
        .expect("drop-in listed");
    assert_eq!(neon.source, "dropin");
    assert!(!neon.active);
    // The bundled defaults stay listed alongside it.
    assert!(
        list.themes
            .iter()
            .any(|t| t.name == "terminal" && t.source == "bundled")
    );

    // theme_get resolves the drop-in merged over the default.
    let got = unwrap_json(mcp.theme_get(get_params(Some("neon"))).await).expect("get neon");
    assert_eq!(got.name, "neon");
    assert_eq!(
        got.tokens.get("--sb-accent").map(String::as_str),
        Some("#00ff88")
    );
    assert_eq!(
        got.tokens.get("--sb-scale").map(String::as_str),
        Some("1.2")
    );
    assert_eq!(
        got.tokens.get("--sb-accent-2"),
        themes::bundled_default().get("--sb-accent-2")
    );
    assert_eq!(got.base_theme, "default");
    assert_eq!(
        got.contract["baseThemes"],
        json!(["default", "paper", "terminal", "topbar"])
    );
    let paths = got.contract["paths"].as_array().expect("contract paths");
    for path in ["/configSchema", "/referenceThemes", "/themeSchema"] {
        assert!(paths.contains(&json!(path)));
    }
    assert!(got.contract["themeFormat"].is_object());

    // Activation goes through settings_set and broadcasts a theme change.
    let mut changes = mcp.config.subscribe();
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "theme".to_string(),
            value: json!("neon"),
        }))
        .await,
    )
    .expect("activate theme");
    assert_eq!(mcp.config.current().theme, "neon");
    let change = changes.try_recv().expect("theme change must broadcast");
    assert!(change.theme_changed());

    // Without a name, theme_get now resolves the active theme.
    let active = unwrap_json(mcp.theme_get(get_params(None)).await).expect("get active");
    assert_eq!(active.name, "neon");
    assert_eq!(
        active.tokens.get("--sb-accent").map(String::as_str),
        Some("#00ff88")
    );

    // Writing the now-active theme points at the re-apply workaround.
    let ack = unwrap_json(
        mcp.theme_write(write_params("neon", json!({ "--sb-accent": "#ff0000" })))
            .await,
    )
    .expect("write active theme");
    assert!(ack.message.contains("ACTIVE"));
    assert!(ack.message.contains("switch to another theme and back"));
    assert!(!ack.message.contains("restart"));
}

#[tokio::test]
async fn full_theme_roundtrip_applies_settings_on_every_activation() {
    let (_dir, mcp) = test_handler().await;

    let ack = unwrap_json(
        mcp.theme_write(write_params_with_settings(
            "dockish",
            json!({ "--sb-accent": "#22c1a7" }),
            json!({ "layout.width": "auto", "appearance.tileChrome": "flat" }),
        ))
        .await,
    )
    .expect("write full theme");
    assert!(ack.message.contains("2 activation setting(s)"));

    // theme_get serves the settings block back.
    let got = unwrap_json(mcp.theme_get(get_params(Some("dockish"))).await).expect("get");
    assert_eq!(got.settings.get("layout.width"), Some(&json!("auto")));
    assert_eq!(
        got.settings.get("appearance.tileChrome"),
        Some(&json!("flat"))
    );

    // Activation applies look AND behavior.
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "theme".to_string(),
            value: json!("dockish"),
        }))
        .await,
    )
    .expect("activate");
    let config = mcp.config.current();
    assert_eq!(config.theme, "dockish");
    assert_eq!(config.layout.width, crate::config::BarWidth::Auto);
    assert_eq!(
        config.appearance.tile_chrome,
        crate::config::TileChrome::Flat
    );

    // The user changes a themed value; re-activating resets it to the theme.
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "layout.width".to_string(),
            value: json!("full"),
        }))
        .await,
    )
    .expect("user override");
    assert_eq!(
        mcp.config.current().layout.width,
        crate::config::BarWidth::Full
    );
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "theme".to_string(),
            value: json!("dockish"),
        }))
        .await,
    )
    .expect("re-activate");
    assert_eq!(
        mcp.config.current().layout.width,
        crate::config::BarWidth::Auto
    );

    // Every bundled base is complete: default resets its visual/bar settings.
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "theme".to_string(),
            value: json!("default"),
        }))
        .await,
    )
    .expect("back to default");
    let after = mcp.config.current();
    assert_eq!(after.theme, "default");
    assert_eq!(after.layout, crate::config::LayoutConfig::default());
    assert_eq!(after.appearance, crate::config::AppearanceConfig::default());
}

#[tokio::test]
async fn theme_write_rejects_invalid_settings_blocks() {
    let (_dir, mcp) = test_handler().await;

    let err = unwrap_json(
        mcp.theme_write(write_params_with_settings(
            "bad",
            json!({}),
            json!({ "language": "de" }),
        ))
        .await,
    )
    .expect_err("disallowed path");
    assert!(err.message.contains("language"));
    assert!(err.message.contains("layout.*"));

    let err = unwrap_json(
        mcp.theme_write(write_params_with_settings(
            "bad",
            json!({}),
            json!({ "layout.position": "sideways" }),
        ))
        .await,
    )
    .expect_err("invalid value");
    assert!(err.message.contains("layout.position"));

    let err = unwrap_json(
        mcp.theme_write(write_params_with_settings(
            "bad",
            json!({}),
            json!("{\"layout.position\":\"top\"}"),
        ))
        .await,
    )
    .expect_err("stringified settings");
    assert!(err.message.contains("real JSON object"));

    assert!(!mcp.paths.themes_dir().join("bad.json").exists());
}

#[tokio::test]
async fn theme_get_rejects_unknown_and_invalid_names() {
    let (_dir, mcp) = test_handler().await;
    let err =
        unwrap_json(mcp.theme_get(get_params(Some("ghost"))).await).expect_err("unknown theme");
    assert!(err.message.contains("ghost"));
    assert!(err.message.contains("default"));

    let err =
        unwrap_json(mcp.theme_get(get_params(Some("../evil"))).await).expect_err("invalid name");
    assert!(err.message.contains("[a-z0-9-]"));
}

#[tokio::test]
async fn theme_write_rejects_bundled_and_bad_names() {
    let (_dir, mcp) = test_handler().await;
    for name in ["default", "paper", "terminal", "topbar"] {
        let err = unwrap_json(mcp.theme_write(write_params(name, json!({}))).await)
            .expect_err("bundled themes are read-only");
        assert!(err.message.contains("read-only"), "{}", err.message);
    }

    for name in ["", "Has-Upper", "under_score", "../up"] {
        let err = unwrap_json(mcp.theme_write(write_params(name, json!({}))).await)
            .expect_err(&format!("name {name:?} must be rejected"));
        assert!(err.message.contains("[a-z0-9-]"), "{}", err.message);
    }
    assert!(!mcp.paths.themes_dir().exists());
}

#[tokio::test]
async fn theme_write_rejects_stringified_objects_and_bad_tokens() {
    let (_dir, mcp) = test_handler().await;

    // A JSON object serialized into a string is a client bug worth naming.
    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!("{\"--sb-accent\":\"#fff\"}")))
            .await,
    )
    .expect_err("stringified object");
    assert!(err.message.contains("pass a real JSON object"));

    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!(["--sb-accent"])))
            .await,
    )
    .expect_err("non-object tokens");
    assert!(err.message.contains("JSON object"));

    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!({ "--sb-accent": 7 })))
            .await,
    )
    .expect_err("non-string value");
    assert!(err.message.contains("string value"));

    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!({ "accent": "#fff" })))
            .await,
    )
    .expect_err("key without -- prefix");
    assert!(err.message.contains("custom-property"));

    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!({ "--sb-not-real": "1rem" })))
            .await,
    )
    .expect_err("unknown smabar token");
    assert!(err.message.contains("not a known public"));

    let err = unwrap_json(
        mcp.theme_write(write_params("neon", json!({ "--sb-avatar-size": "3rem" })))
            .await,
    )
    .expect_err("component-local token");
    assert!(err.message.contains("component-local"));

    for value in [
        "rgba(0, 0, 0, .5)",
        "color-mix(in srgb, rgba(0, 0, 0, .5), white)",
        "var(--plugin-surface)",
    ] {
        let err = unwrap_json(
            mcp.theme_write(write_params("neon", json!({ "--sb-bar-bg": value })))
                .await,
        )
        .expect_err("transparent or unsafe surface");
        assert!(err.message.contains("must be opaque"));
    }

    for value in [
        "red; display: none",
        "red } body {",
        "url(javascript:alert(1))",
    ] {
        let err = unwrap_json(
            mcp.theme_write(write_params("neon", json!({ "--sb-accent": value })))
                .await,
        )
        .expect_err(&format!("value {value:?} must be rejected"));
        assert!(err.message.contains("--sb-accent"), "{}", err.message);
    }
    // Nothing may have been written for the rejected calls.
    assert!(!mcp.paths.themes_dir().join("neon.json").exists());
}
