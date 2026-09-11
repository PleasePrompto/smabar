//! One-shot application of a theme's behavior settings block.
//!
//! A theme drop-in may carry a reserved [`super::SETTINGS_KEY`] object of
//! dotted config paths. Activating the theme (setting the `theme` config
//! path via the settings panel or the MCP `settings_set` tool) applies that
//! block once through [`crate::config::update::set_config_path`]; afterwards
//! the user is free to change anything — re-activating the theme resets to
//! the theme's values again. External config-file edits never trigger it.

use serde_json::Value;

use crate::config::update::set_config_path;
use crate::config::{SmabarConfig, SmabarPaths};

use super::ThemeSettings;

/// Exact config paths a theme settings block may set. Keeping this list exact
/// makes the theme contract and runtime validation agree; in particular,
/// `appearance.tokens` is deliberately excluded because tokens have their
/// own top-level channel in the theme document.
pub const ALLOWED_PATHS: [&str; 23] = [
    "zOrder",
    "layout.position",
    "layout.variant",
    "layout.dividerRatio",
    "layout.primaryZone",
    "layout.width",
    "layout.maxWidth",
    "layout.margin",
    "layout.behavior",
    "layout.yieldToFullscreen",
    "appearance.barChrome",
    "appearance.tileChrome",
    "appearance.shortcutAlign",
    "appearance.pluginAlign",
    "effects.hoverMagnify.enabled",
    "effects.hoverMagnify.scale",
    "effects.hoverMagnify.neighbors",
    "effects.hoverPeek.enabled",
    "effects.hoverPeek.delayMs",
    "popups.position",
    "shortcuts.labels",
    "shortcuts.iconSize",
    "shortcuts.labelSize",
];

/// Is this dotted config path allowed inside a theme settings block?
///
/// Deliberately excluded: `theme` (recursion), `language`, `mcp`,
/// `plugins`, `pluginOrder`, `pluginsHidden`, and
/// `shortcuts.pinned` — a shared theme must never change the user's pins,
/// plugins or language.
pub fn is_allowed_setting_path(path: &str) -> bool {
    ALLOWED_PATHS.contains(&path)
}

/// Human-readable list of what a settings block may contain (for errors and
/// tool descriptions).
pub const ALLOWED_SUMMARY: &str = "layout.* except layout.monitor, appearance.barChrome/tileChrome/shortcutAlign/pluginAlign, \
     effects.*, shortcuts.labels/iconSize/labelSize, zOrder, popups.position (tokens belong in \
     the theme's top-level --sb-* map, never appearance.tokens)";

/// Applies `settings` onto `config`, entry by entry. Invalid entries are
/// skipped with an actionable message; valid entries still apply — a theme
/// with one bad entry keeps working.
pub fn apply_settings(
    config: SmabarConfig,
    settings: &ThemeSettings,
) -> (SmabarConfig, Vec<String>) {
    let mut current = config;
    let mut errors = Vec::new();
    for (path, value) in settings {
        if !is_allowed_setting_path(path) {
            errors.push(format!(
                "theme setting \"{path}\" is not allowed (themes may set: {ALLOWED_SUMMARY})"
            ));
            continue;
        }
        match set_config_path(&current, path, value.clone()) {
            Ok(next) => current = next,
            Err(error) => errors.push(format!("theme setting \"{path}\": {error}")),
        }
    }
    (current, errors)
}

/// One-shot activation of `config.theme`: reads the theme's settings block
/// and applies it. Errors are also logged here so a Tauri-side activation
/// leaves a trace without extra plumbing.
pub fn activate(paths: &SmabarPaths, config: SmabarConfig) -> (SmabarConfig, Vec<String>) {
    let theme = config.theme.clone();
    let block = super::settings_block(paths, &theme);
    // A theme switch starts from its own canonical token layer. Keeping old
    // appearance overrides here made a newly selected base theme look only
    // partly selected until every slider was reset by hand.
    let mut config = config;
    config.appearance.tokens.clear();
    let (config, errors) = apply_settings(config, &block);
    for error in &errors {
        tracing::warn!(theme, error, "skipped invalid theme setting on activation");
    }
    (config, errors)
}

/// Validates a settings block for `theme_write`: every path must be allowed
/// and every value must apply cleanly to a default config (same schema
/// validation as a live `settings_set`).
pub fn validate_settings(settings: &ThemeSettings) -> Result<(), Vec<String>> {
    let (_, errors) = apply_settings(SmabarConfig::default(), settings);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Parses the untyped `settings` value of a `theme_write` call into a
/// [`ThemeSettings`] block. Objects arriving as JSON-encoded strings get a
/// pointed message (some MCP clients serialize nested objects that way).
pub fn parse_settings_value(settings: Value) -> Result<ThemeSettings, String> {
    match settings {
        Value::Object(block) => Ok(block.into_iter().collect()),
        Value::String(raw) if serde_json::from_str::<Value>(&raw).is_ok_and(|v| v.is_object()) => {
            Err("`settings` arrived as a JSON-encoded string; pass a real JSON object".to_string())
        }
        _ => Err(
            "`settings` must be a JSON object mapping dotted config paths to values".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::config::{BarPosition, SmabarPaths, TileChrome, ZOrder};

    use super::*;

    fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        (dir, paths)
    }

    fn write_theme(paths: &SmabarPaths, name: &str, content: &str) {
        let dir = paths.themes_dir();
        std::fs::create_dir_all(&dir).expect("create themes dir");
        std::fs::write(dir.join(format!("{name}.json")), content).expect("write theme");
    }

    #[test]
    fn allowlist_accepts_documented_paths_and_rejects_the_rest() {
        for path in [
            "layout.position",
            "layout.behavior",
            "appearance.tileChrome",
            "effects.hoverMagnify.scale",
            "shortcuts.labels",
            "shortcuts.iconSize",
            "shortcuts.labelSize",
            "zOrder",
            "popups.position",
        ] {
            assert!(is_allowed_setting_path(path), "{path} must be allowed");
        }
        for path in [
            "theme",
            "language",
            "mcp.port",
            "plugins.hello.city",
            "pluginOrder",
            "pluginsHidden",
            "shortcuts.pinned",
            "shortcuts",
            "popups.enabled",
            "layout", // whole-section replacement is not allowed
            "layout.monitor",
            "appearance.tokens",
            "appearance.tokens.--sb-accent",
            "appearance.futureOption",
            "effects.futureOption",
            "",
        ] {
            assert!(!is_allowed_setting_path(path), "{path} must be rejected");
        }
    }

    #[test]
    fn apply_settings_applies_valid_entries_and_reports_the_rest() {
        let settings = ThemeSettings::from([
            ("layout.position".to_string(), json!("top")),
            ("appearance.tileChrome".to_string(), json!("flat")),
            ("zOrder".to_string(), json!("bottom")),
            // Not allowed: reported, not applied.
            ("language".to_string(), json!("de")),
            // Allowed path, invalid value: reported, not applied.
            ("layout.variant".to_string(), json!("diagonal")),
        ]);
        let (config, errors) = apply_settings(SmabarConfig::default(), &settings);
        assert_eq!(config.layout.position, BarPosition::Top);
        assert_eq!(config.appearance.tile_chrome, TileChrome::Flat);
        assert_eq!(config.z_order, ZOrder::Bottom);
        assert_eq!(config.language, "en");
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|e| e.contains("\"language\"")));
        assert!(errors.iter().any(|e| e.contains("\"layout.variant\"")));
    }

    #[test]
    fn activating_the_default_theme_changes_nothing() {
        let (_dir, paths) = temp_paths();
        let config = SmabarConfig::default();
        let (activated, errors) = activate(&paths, config.clone());
        assert_eq!(activated, config);
        assert!(errors.is_empty());
    }

    #[test]
    fn activation_clears_stale_appearance_token_overrides() {
        let (_dir, paths) = temp_paths();
        let mut config = SmabarConfig::default();
        config
            .appearance
            .tokens
            .insert("--sb-accent".to_string(), "#00ff88".to_string());

        let (activated, errors) = activate(&paths, config);
        assert!(errors.is_empty());
        assert!(activated.appearance.tokens.is_empty());
    }

    #[test]
    fn activation_reapplies_the_theme_over_user_changes() {
        let (_dir, paths) = temp_paths();
        write_theme(
            &paths,
            "full",
            r##"{"--sb-accent":"#00ff88","settings":{"layout.position":"top"}}"##,
        );
        let mut config = SmabarConfig {
            theme: "full".to_string(),
            ..SmabarConfig::default()
        };
        let (activated, errors) = activate(&paths, config.clone());
        assert_eq!(activated.layout.position, BarPosition::Top);
        assert!(errors.is_empty());

        // The user flips it back; re-activating restores the theme's value.
        config.layout.position = BarPosition::Bottom;
        let (reactivated, _) = activate(&paths, config);
        assert_eq!(reactivated.layout.position, BarPosition::Top);
    }

    #[test]
    fn validate_settings_reports_every_problem() {
        assert!(validate_settings(&ThemeSettings::new()).is_ok());
        assert!(
            validate_settings(&ThemeSettings::from([(
                "layout.position".to_string(),
                json!("top")
            )]))
            .is_ok()
        );
        let errors = validate_settings(&ThemeSettings::from([
            ("language".to_string(), json!("de")),
            ("layout.position".to_string(), json!("sideways")),
        ]))
        .expect_err("both entries are invalid");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn parse_settings_value_accepts_objects_and_names_client_bugs() {
        let block = parse_settings_value(json!({ "layout.position": "top" })).expect("object");
        assert_eq!(block.get("layout.position"), Some(&json!("top")));

        let err = parse_settings_value(json!("{\"layout.position\":\"top\"}"))
            .expect_err("stringified object");
        assert!(err.contains("JSON-encoded string"));

        let err = parse_settings_value(json!(["layout.position"])).expect_err("array");
        assert!(err.contains("JSON object"));
    }
}
