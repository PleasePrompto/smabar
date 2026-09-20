//! Tests for the bundled-theme registry, drop-in resolution, and pickers.

use std::fs;

use super::*;

fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    (dir, paths)
}

fn write_theme(paths: &SmabarPaths, name: &str, content: &str) {
    let dir = paths.themes_dir();
    fs::create_dir_all(&dir).expect("create themes dir");
    fs::write(dir.join(format!("{name}.json")), content).expect("write theme");
}

#[test]
fn every_bundled_theme_parses_and_validates() {
    assert_eq!(
        BUNDLED.len(),
        4,
        "the curated bundled set must stay complete"
    );
    let default_keys: Vec<_> = bundled_default().keys().cloned().collect();
    for (name, theme) in BUNDLED.iter() {
        let map = &theme.tokens;
        let raw = BUNDLED_JSON
            .iter()
            .find(|(raw_name, _)| raw_name == name)
            .and_then(|(_, json)| {
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json).ok()
            })
            .expect("bundled source parses");
        let raw_token_count = raw
            .keys()
            .filter(|key| key.as_str() != SETTINGS_KEY)
            .count();
        assert_eq!(
            map.len(),
            raw_token_count,
            "bundled theme {name} lost invalid tokens"
        );
        // Every bundle must pass its own validation rules …
        for (key, value) in map {
            assert!(is_valid_token_key(key), "{name}: key {key:?} invalid");
            assert!(
                is_valid_token_value(value),
                "{name}: value for {key:?} invalid"
            );
        }
        if *name != "default" {
            // … and be COMPLETE: a non-default theme missing a key would
            // silently inherit the dark-violet default fallback at that spot.
            for key in &default_keys {
                assert!(
                    map.contains_key(key),
                    "bundled theme {name} is missing token {key}"
                );
            }
        }
        assert_eq!(
            theme.settings.len(),
            settings::ALLOWED_PATHS.len(),
            "bundled theme {name} must define every allowed visual/bar setting"
        );
        settings::validate_settings(&theme.settings)
            .unwrap_or_else(|errors| panic!("bundled theme {name}: {}", errors.join("; ")));
    }
}

#[test]
fn canonical_metadata_matches_the_default_reference_exactly() {
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("../../../../ui-kit/theme-contract.json"))
            .expect("theme contract parses");
    let definitions = contract["baseTokens"].as_object().expect("baseTokens");
    assert_eq!(definitions.len(), bundled_default().len());
    for (name, value) in bundled_default() {
        assert_eq!(definitions[name]["default"], value.as_str(), "{name}");
        for field in [
            "type",
            "allowed",
            "meaning",
            "group",
            "dependencies",
            "consumers",
        ] {
            assert!(!definitions[name][field].is_null(), "{name} misses {field}");
        }
    }
    let documented_paths = contract["settings"]["allowedPaths"]
        .as_object()
        .expect("allowed settings");
    assert_eq!(documented_paths.len(), settings::ALLOWED_PATHS.len());
    for path in settings::ALLOWED_PATHS {
        assert!(
            documented_paths.contains_key(path),
            "metadata misses {path}"
        );
    }
}

#[test]
fn bundled_names_are_plain_and_lookup_works() {
    for name in ["default", "paper", "terminal", "topbar"] {
        assert!(is_bundled(name), "{name} must be bundled");
    }
    for name in ["carbon", "graphite", "linen"] {
        assert!(!is_bundled(name), "{name} must no longer be bundled");
    }
    assert!(!is_bundled("neon"));
}

#[test]
fn resolve_default_returns_the_bundle() {
    let (_dir, paths) = temp_paths();
    assert_eq!(&resolve(&paths, "default"), bundled_default());
}

#[test]
fn resolve_merges_dropin_over_default_and_keeps_unknown_tokens() {
    let (_dir, paths) = temp_paths();
    write_theme(
        &paths,
        "neon",
        r##"{"--sb-accent":"#00ff88","--my-plugin-glow":"0 0 8px #00ff88"}"##,
    );

    let map = resolve(&paths, "neon");
    assert_eq!(map.get("--sb-accent").map(String::as_str), Some("#00ff88"));
    // Themes may invent tokens beyond the bundled set (for plugins).
    assert_eq!(
        map.get("--my-plugin-glow").map(String::as_str),
        Some("0 0 8px #00ff88")
    );
    // Tokens missing from the drop-in fall back to the default.
    assert_eq!(
        map.get("--sb-accent-2"),
        bundled_default().get("--sb-accent-2")
    );
    assert_eq!(map.len(), bundled_default().len() + 1);
}

#[test]
fn resolve_bases_nondefault_bundles_on_their_own_map() {
    let (_dir, paths) = temp_paths();
    // A complete bundled light/dark variant resolves to itself, not to the
    // bundled default.
    let terminal = resolve(&paths, "terminal");
    assert_eq!(
        terminal.get("--sb-text").map(String::as_str),
        Some("#d9dcd6")
    );
}

#[test]
fn same_named_dropin_patches_a_bundled_theme_per_token() {
    let (_dir, paths) = temp_paths();
    write_theme(&paths, "terminal", r##"{"--sb-accent":"#00ff88"}"##);

    let map = resolve(&paths, "terminal");
    assert_eq!(map.get("--sb-accent").map(String::as_str), Some("#00ff88"));
    // The rest stays the bundled terminal look.
    assert_eq!(map.get("--sb-text"), Some(&"#d9dcd6".to_string()));
}

#[test]
fn resolve_falls_back_to_default_on_broken_dropin() {
    let (_dir, paths) = temp_paths();
    write_theme(&paths, "neon", "{ not json");
    assert_eq!(&resolve(&paths, "neon"), bundled_default());

    // Structurally valid JSON but not a flat string map is also "broken".
    write_theme(&paths, "nested", r#"{"--sb-accent":{"deep":true}}"#);
    assert_eq!(&resolve(&paths, "nested"), bundled_default());
}

#[test]
fn resolve_falls_back_to_default_for_missing_or_invalid_name() {
    let (_dir, paths) = temp_paths();
    assert_eq!(&resolve(&paths, "ghost"), bundled_default());
    assert_eq!(&resolve(&paths, "../evil"), bundled_default());
    assert_eq!(&resolve(&paths, ""), bundled_default());
}

#[test]
fn resolve_skips_invalid_tokens_from_dropin() {
    let (_dir, paths) = temp_paths();
    write_theme(
        &paths,
        "sneaky",
        r##"{
                "--sb-accent": "#00ff88",
                "--sb-text": "red; display: none",
                "not-a-custom-property": "red",
                "--sb-danger": "url(javascript:alert(1))"
            }"##,
    );

    let map = resolve(&paths, "sneaky");
    assert_eq!(map.get("--sb-accent").map(String::as_str), Some("#00ff88"));
    assert_eq!(map.get("--sb-text"), bundled_default().get("--sb-text"));
    assert_eq!(map.get("--sb-danger"), bundled_default().get("--sb-danger"));
    assert!(!map.contains_key("not-a-custom-property"));
}

#[test]
fn resolve_skips_transparent_surface_and_unknown_smabar_tokens() {
    let (_dir, paths) = temp_paths();
    write_theme(
        &paths,
        "unsafe",
        r##"{
            "--sb-bar-bg": "rgba(38, 22, 12, 0.6)",
            "--sb-future-guess": "1rem",
            "--plugin-card-bg": "#26160c"
        }"##,
    );

    let map = resolve(&paths, "unsafe");
    assert_eq!(map.get("--sb-bar-bg"), bundled_default().get("--sb-bar-bg"));
    assert!(!map.contains_key("--sb-future-guess"));
    assert_eq!(map.get("--plugin-card-bg"), Some(&"#26160c".to_string()));
}

#[test]
fn foreign_google_font_sources_are_allowlisted_and_canonicalized() {
    let (_dir, paths) = temp_paths();
    write_theme(
        &paths,
        "foreign",
        r##"{
            "--sb-font-sans": "Wrong Family, fantasy",
            "--sb-font-sans-source": "google:roboto"
        }"##,
    );
    let known = resolve(&paths, "foreign");
    assert_eq!(
        known.get("--sb-font-sans").map(String::as_str),
        Some("'Roboto', system-ui, sans-serif")
    );
    assert_eq!(
        known.get("--sb-font-sans-source").map(String::as_str),
        Some("google:roboto")
    );

    write_theme(
        &paths,
        "unknown",
        r##"{
            "--sb-font-sans": "Local Theme Font, sans-serif",
            "--sb-font-sans-source": "https://example.com/font.woff2"
        }"##,
    );
    let unknown = resolve(&paths, "unknown");
    assert_eq!(
        unknown.get("--sb-font-sans-source"),
        bundled_default().get("--sb-font-sans-source")
    );
    // The harmless family stack still works when that font exists locally;
    // the invalid source can never trigger a network request.
    assert_eq!(
        unknown.get("--sb-font-sans").map(String::as_str),
        Some("Local Theme Font, sans-serif")
    );
}

#[test]
fn settings_key_is_reserved_and_never_a_token() {
    let (_dir, paths) = temp_paths();
    write_theme(
        &paths,
        "full",
        r##"{
                "--sb-accent": "#00ff88",
                "settings": { "layout.position": "top", "shortcuts.iconSize": 48 }
            }"##,
    );

    // The settings block never leaks into the token map …
    let map = resolve(&paths, "full");
    assert!(!map.contains_key(SETTINGS_KEY));
    assert_eq!(map.get("--sb-accent").map(String::as_str), Some("#00ff88"));

    // … and is served separately, in full.
    let block = settings_block(&paths, "full");
    assert_eq!(
        block.get("layout.position"),
        Some(&serde_json::json!("top"))
    );
    assert_eq!(block.len(), 2);
    assert!(!is_valid_token_key(SETTINGS_KEY));
}

#[test]
fn settings_block_is_empty_without_a_block_and_for_broken_blocks() {
    let (_dir, paths) = temp_paths();
    assert_eq!(
        settings_block(&paths, "default").len(),
        settings::ALLOWED_PATHS.len()
    );
    assert_eq!(
        settings_block(&paths, "paper").len(),
        settings::ALLOWED_PATHS.len()
    );
    assert!(settings_block(&paths, "ghost").is_empty());

    write_theme(&paths, "plain", r##"{"--sb-accent":"#00ff88"}"##);
    assert!(settings_block(&paths, "plain").is_empty());

    // A non-object `settings` value is ignored; the tokens still apply. Only
    // theme_write validates the block up front, so a hand-edited file is the
    // one thing that reaches this branch.
    write_theme(
        &paths,
        "odd",
        r##"{"--sb-accent":"#00ff88","settings":["layout.position"]}"##,
    );
    assert!(settings_block(&paths, "odd").is_empty());
    assert_eq!(
        resolve(&paths, "odd")
            .get("--sb-accent")
            .map(String::as_str),
        Some("#00ff88")
    );
}

#[test]
fn available_themes_lists_bundled_plus_dropins_without_duplicates() {
    let (_dir, paths) = temp_paths();
    let bundled = vec![
        "default".to_string(),
        "paper".to_string(),
        "terminal".to_string(),
        "topbar".to_string(),
    ];
    assert_eq!(available_themes(&paths), bundled);

    write_theme(&paths, "neon", "{}");
    write_theme(&paths, "quartz", "{}");
    write_theme(&paths, "terminal", "{}"); // patches the bundled one
    fs::write(paths.themes_dir().join("notes.txt"), "ignore me").expect("write txt");

    assert_eq!(
        available_themes(&paths),
        vec!["default", "neon", "paper", "quartz", "terminal", "topbar"]
    );
}

#[test]
fn summaries_expose_source_active_and_preview_colors() {
    let (_dir, paths) = temp_paths();
    write_theme(&paths, "neon", r##"{"--sb-accent":"#00ff88"}"##);

    let infos = summaries(
        &paths,
        &crate::config::SmabarConfig {
            theme: "paper".into(),
            ..Default::default()
        },
    );
    let paper = infos
        .iter()
        .find(|info| info.name == "paper")
        .expect("bundled listed");
    assert_eq!(paper.source, "bundled");
    assert!(paper.active);
    assert_eq!(paper.colors.accent, "#2f5fa8");
    assert_eq!(paper.colors.surface, "#f4f2ed");
    assert_eq!(
        paper.preview.layout.position,
        crate::config::BarPosition::Top
    );
    assert_eq!(
        paper.preview.appearance.tokens["--sb-bar-radius"],
        "0.625rem"
    );
    assert_eq!(
        paper.fonts.sans.family,
        "system-ui, -apple-system, 'Segoe UI', Roboto, 'Noto Sans', Ubuntu, Cantarell, sans-serif"
    );
    assert_eq!(paper.fonts.sans.source, "system");
    assert_eq!(paper.fonts.mono.source, "system");

    let neon = infos
        .iter()
        .find(|info| info.name == "neon")
        .expect("dropin listed");
    assert_eq!(neon.source, "dropin");
    assert!(!neon.active);
    // Drop-ins resolve over the default, so preview colors reflect reality.
    assert_eq!(neon.colors.accent, "#00ff88");
    assert_eq!(
        neon.colors.text.as_str(),
        bundled_default()
            .get("--sb-text")
            .map(String::as_str)
            .expect("text token")
    );
}

#[test]
fn preview_inherits_omitted_settings_without_changing_the_current_look() {
    use crate::config::{BarPosition, BarVariant, LayoutBehavior, SmabarConfig};
    let (_dir, paths) = temp_paths();
    let mut current = SmabarConfig::default();
    current.layout.position = BarPosition::Top;
    current.layout.margin = 24;
    current
        .appearance
        .tokens
        .insert("--sb-accent".into(), "#abcdef".into());
    write_theme(
        &paths,
        "custom",
        r##"{
        "--sb-accent": "#123456",
        "settings": {"layout.behavior":"autohide", "layout.variant":"rows"}
    }"##,
    );
    let original = current.clone();
    let themes = summaries(&paths, &current);
    let preview = &themes
        .iter()
        .find(|theme| theme.name == "custom")
        .expect("custom")
        .preview;
    assert_eq!(preview.layout.position, BarPosition::Top);
    assert_eq!(preview.layout.margin, 24);
    assert_eq!(preview.layout.behavior, LayoutBehavior::Autohide);
    assert_eq!(preview.layout.variant, BarVariant::Rows);
    assert_eq!(preview.appearance.tokens["--sb-accent"], "#123456");
    assert_eq!(current, original);
}

#[test]
fn theme_name_validation_rejects_paths_and_uppercase() {
    assert!(is_valid_theme_name("neon-2"));
    for name in ["", "Neon", "under_score", "../up", "a.b", "a b"] {
        assert!(!is_valid_theme_name(name), "name {name:?} must be invalid");
    }
}

#[test]
fn token_key_validation_requires_custom_property_names() {
    assert!(is_valid_token_key("--sb-accent"));
    assert!(is_valid_token_key("--my_plugin-Token2"));
    for key in ["", "--", "sb-accent", "--sb accent", "--sb;accent"] {
        assert!(!is_valid_token_key(key), "key {key:?} must be invalid");
    }
}

#[test]
fn token_value_validation_rejects_injection_shaped_values() {
    assert!(is_valid_token_value("#8b5cf6"));
    assert!(is_valid_token_value("blur(28px) saturate(160%)"));
    assert!(is_valid_token_value("none"));
    assert!(is_valid_token_value(
        "linear-gradient(135deg, rgba(139, 92, 246, 0.7), rgba(236, 72, 153, 0.7))"
    ));
    // Semicolons are rejected everywhere, so data: URIs must use the
    // percent-encoded form (`%3B`) instead of `;base64`.
    assert!(is_valid_token_value("url(data:image/svg+xml,%3Csvg%3E)"));
    assert!(is_valid_token_value("url('https://example.com/x.png')"));
    for value in [
        "",
        "red; display: none",
        "red } body { color: blue",
        "red\nblue",
        "url(javascript:alert(1))",
        "url(http://example.com/x.png)",
        "URL( javascript:alert(1))",
    ] {
        assert!(
            !is_valid_token_value(value),
            "value {value:?} must be invalid"
        );
    }
}
