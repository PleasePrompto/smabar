//! Dotted-path config updates, shared by the MCP `settings_set` tool and the
//! Tauri `update_config` command (parity by construction).

use serde_json::Value;
use thiserror::Error;

use super::SmabarConfig;

/// Top-level config fields a dotted-path update may touch; a typo'd first
/// segment is rejected instead of silently landing as an ignored extra field.
pub const SETTABLE_ROOTS: [&str; 17] = [
    "zOrder",
    "language",
    "theme",
    "themeExportDir",
    "pluginOrder",
    "plugins",
    "mcp",
    "rendering",
    "layout",
    "shortcuts",
    "pluginsHidden",
    "pluginsDeactivated",
    "effects",
    "appearance",
    "popups",
    "audio",
    "settingsWindow",
];

/// Errors from [`set_config_path`]. All variants are caller mistakes
/// (invalid path or value) except [`SetPathError::Serialize`].
#[derive(Debug, Error)]
pub enum SetPathError {
    /// The first path segment is not a settable root.
    #[error("config path \"{path}\" must start with one of: {roots}", roots = SETTABLE_ROOTS.join(", "))]
    UnknownRoot { path: String },
    /// The path is empty or contains an empty segment (`a..b`).
    #[error("config path \"{path}\" contains an empty segment")]
    EmptySegment { path: String },
    /// A parent segment exists but is not a JSON object.
    #[error("\"{segment}\" in \"{path}\" is not an object")]
    NotAnObject { segment: String, path: String },
    /// A parent segment does not exist (intermediates are only created
    /// under `plugins`).
    #[error("no value at \"{segment}\" in config path \"{path}\"")]
    MissingSegment { segment: String, path: String },
    /// The value is a string containing serialized JSON — some MCP clients
    /// serialize the untyped `value` parameter that way, and the string
    /// would land in the config unnoticed while the consumer never sees its
    /// object. Deliberate tradeoff: a genuine string that itself parses as a
    /// JSON object/array is rejected too — for agent clients the silent trap
    /// is the greater evil.
    #[error(
        "value for \"{path}\" arrived as a JSON-encoded string; pass the JSON object/array \
         itself, not its string form"
    )]
    JsonEncodedString { path: String },
    /// The updated config no longer matches the schema.
    #[error("value for \"{path}\" was rejected: {source}")]
    Rejected {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    /// The JSON shape is valid but a value is outside the supported contract.
    #[error("value for \"{path}\" was rejected: {source}")]
    InvalidValue {
        path: String,
        #[source]
        source: super::ConfigValidationError,
    },
    /// An `appearance.tokens` entry violates the shared theme contract.
    #[error("appearance.tokens entry {key:?} = {value:?} is invalid: {reason}")]
    InvalidToken {
        key: String,
        value: String,
        reason: String,
    },
    /// A theme name is well-formed but no bundled/drop-in theme provides it.
    #[error(
        "theme \"{name}\" is not installed; create it with theme_write/import or choose an installed theme"
    )]
    UnknownTheme { name: String },
    /// Serializing the current config failed (internal error).
    #[error("cannot serialize config: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
}

/// Returns a copy of `current` with `value` set at the dotted `path`
/// (e.g. `layout.position` or `plugins.hello.city`). Missing intermediate
/// objects are created only under `plugins` and `audio.plugins`. The result is schema-validated;
/// persisting it is the caller's job.
pub fn set_config_path(
    current: &SmabarConfig,
    path: &str,
    value: Value,
) -> Result<SmabarConfig, SetPathError> {
    let first = path.split('.').next().unwrap_or_default();
    if !SETTABLE_ROOTS.contains(&first) {
        return Err(SetPathError::UnknownRoot {
            path: path.to_string(),
        });
    }
    reject_json_encoded_string(path, &value)?;
    let mut root =
        serde_json::to_value(current).map_err(|source| SetPathError::Serialize { source })?;
    set_at_path(
        &mut root,
        path,
        value,
        first == "plugins" || path.starts_with("audio.plugins."),
    )?;
    let mut updated: SmabarConfig =
        serde_json::from_value(root).map_err(|source| SetPathError::Rejected {
            path: path.to_string(),
            source,
        })?;
    crate::fonts::canonicalize_theme_fonts(&mut updated.appearance.tokens);
    updated
        .validate()
        .map_err(|source| SetPathError::InvalidValue {
            path: path.to_string(),
            source,
        })?;
    validate_tokens(&updated)?;
    Ok(updated)
}

/// Rejects `appearance.tokens` entries the stylesheet cannot use. Same rules
/// as theme drop-ins (`crate::themes`), so both layers accept exactly the
/// same values.
fn validate_tokens(config: &SmabarConfig) -> Result<(), SetPathError> {
    for (key, value) in &config.appearance.tokens {
        if let Err(reason) = crate::themes::contract::validate_token(key, value) {
            return Err(SetPathError::InvalidToken {
                key: key.clone(),
                value: value.clone(),
                reason,
            });
        }
    }
    Ok(())
}

/// Like [`set_config_path`], but when `path` is `theme` the named theme's
/// behavior settings block is applied one-shot on top (see
/// [`crate::themes::settings`]). This is THE write path of the MCP
/// `settings_set` tool and the Tauri `update_config` command — theme
/// activation behaves identically in both. Returned strings are actionable
/// messages for settings-block entries that could not be applied; the theme
/// itself still activates.
pub fn set_config_path_activating(
    paths: &crate::config::SmabarPaths,
    current: &SmabarConfig,
    path: &str,
    value: Value,
) -> Result<(SmabarConfig, Vec<String>), SetPathError> {
    // Theme activation discards the override layer. Clear it before the
    // ordinary config validator too, so an invalid stale override cannot
    // prevent the user from switching themes.
    let mut base = current.clone();
    if path == "theme" {
        base.appearance.tokens.clear();
    }
    let updated = set_config_path(&base, path, value)?;
    if path == "theme" {
        if !crate::themes::is_available(paths, &updated.theme) {
            return Err(SetPathError::UnknownTheme {
                name: updated.theme,
            });
        }
        Ok(crate::themes::settings::activate(paths, updated))
    } else {
        Ok((updated, Vec::new()))
    }
}

/// Reads the value at a dotted `path` in a JSON tree (empty segments miss).
pub fn get_config_path<'v>(root: &'v Value, path: &str) -> Option<&'v Value> {
    let mut current = root;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// See [`SetPathError::JsonEncodedString`].
fn reject_json_encoded_string(path: &str, value: &Value) -> Result<(), SetPathError> {
    let Value::String(text) = value else {
        return Ok(());
    };
    let trimmed = text.trim_start();
    let looks_like_json = trimmed.starts_with('{') || trimmed.starts_with('[');
    if looks_like_json
        && serde_json::from_str::<Value>(text)
            .is_ok_and(|parsed| parsed.is_object() || parsed.is_array())
    {
        return Err(SetPathError::JsonEncodedString {
            path: path.to_string(),
        });
    }
    Ok(())
}

/// Sets `value` at the dotted `path` in `root`. Missing intermediate objects
/// are created only when `create_missing` (i.e. under `plugins` or `audio.plugins`).
fn set_at_path(
    root: &mut Value,
    path: &str,
    value: Value,
    create_missing: bool,
) -> Result<(), SetPathError> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(SetPathError::EmptySegment {
            path: path.to_string(),
        });
    }
    // `segments` is non-empty: split always yields at least one element and
    // an empty path was rejected above as an empty segment.
    let Some((last, parents)) = segments.split_last() else {
        return Err(SetPathError::EmptySegment {
            path: path.to_string(),
        });
    };
    let mut current = root;
    for segment in parents {
        let object = current
            .as_object_mut()
            .ok_or_else(|| SetPathError::NotAnObject {
                segment: (*segment).to_string(),
                path: path.to_string(),
            })?;
        if !object.contains_key(*segment) {
            if !create_missing {
                return Err(SetPathError::MissingSegment {
                    segment: (*segment).to_string(),
                    path: path.to_string(),
                });
            }
            object.insert(
                (*segment).to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        current = object
            .get_mut(*segment)
            .ok_or_else(|| SetPathError::MissingSegment {
                segment: (*segment).to_string(),
                path: path.to_string(),
            })?;
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| SetPathError::NotAnObject {
            segment: (*last).to_string(),
            path: path.to_string(),
        })?;
    object.insert((*last).to_string(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{BarPosition, BarVariant};
    use super::*;

    #[test]
    fn sets_nested_values_and_validates_the_schema() {
        let config = SmabarConfig::default();
        let updated = set_config_path(&config, "layout.position", json!("top")).expect("set");
        assert_eq!(updated.layout.position, BarPosition::Top);
        // Untouched siblings survive.
        assert_eq!(updated.layout.variant, BarVariant::Split);

        let updated = set_config_path(&config, "shortcuts.labels", json!("hidden")).expect("set");
        assert_eq!(updated.shortcuts.labels, super::super::LabelMode::Hidden);
        let updated = set_config_path(&config, "shortcuts.iconSize", json!(48)).expect("set");
        assert_eq!(updated.shortcuts.icon_size, 48);

        let updated =
            set_config_path(&config, "appearance.tileChrome", json!("flat")).expect("set");
        assert_eq!(
            updated.appearance.tile_chrome,
            super::super::TileChrome::Flat
        );
        let updated = set_config_path(&config, "popups.enabled", json!(false)).expect("set");
        assert!(!updated.popups.enabled);
        let updated = set_config_path(&config, "popups.position", json!("top-left")).expect("set");
        assert_eq!(
            updated.popups.position,
            super::super::PopupPosition::TopLeft
        );
        let err = set_config_path(&config, "popups.position", json!("middle"))
            .expect_err("invalid popup position");
        assert!(matches!(err, SetPathError::Rejected { .. }));
        let updated =
            set_config_path(&config, "effects.hoverPeek.delayMs", json!(900)).expect("set");
        assert_eq!(updated.effects.hover_peek.delay_ms, 900);
        let updated = set_config_path(&config, "settingsWindow.width", json!(960)).expect("set");
        assert_eq!(updated.settings_window.width, 960);

        let updated = set_config_path(&config, "pluginsHidden", json!(["plugin:clock:clock"]))
            .expect("set list");
        assert_eq!(
            updated.plugins_hidden,
            vec!["plugin:clock:clock".to_string()]
        );

        // Deactivation is the other axis: plugin ids, not tile ids. Setting
        // one must not touch the other.
        let updated =
            set_config_path(&updated, "pluginsDeactivated", json!(["clock"])).expect("set list");
        assert_eq!(updated.plugins_deactivated, vec!["clock".to_string()]);
        assert_eq!(
            updated.plugins_hidden,
            vec!["plugin:clock:clock".to_string()]
        );
    }

    #[test]
    fn rejects_typo_roots_empty_segments_and_bad_values() {
        let config = SmabarConfig::default();

        let err = set_config_path(&config, "moode", json!("x")).expect_err("typo root");
        assert!(err.to_string().contains("must start with one of"));
        assert!(err.to_string().contains("layout"));

        let err = set_config_path(&config, "layout..position", json!("top")).expect_err("empty");
        assert!(matches!(err, SetPathError::EmptySegment { .. }));

        let err = set_config_path(&config, "layout.position", json!("sideways"))
            .expect_err("invalid enum");
        assert!(matches!(err, SetPathError::Rejected { .. }));

        let err = set_config_path(&config, "appearance.tileChrome", json!("raised"))
            .expect_err("invalid tile chrome");
        assert!(matches!(err, SetPathError::Rejected { .. }));

        // Intermediates are only created under plugins.
        let err = set_config_path(&config, "mcp.nested.thing", json!(1)).expect_err("missing");
        assert!(matches!(err, SetPathError::MissingSegment { .. }));
        let err = set_config_path(&config, "mcp.poort", json!(7627)).expect_err("nested typo");
        assert!(matches!(err, SetPathError::Rejected { .. }));
        let err = set_config_path(&config, "mcp.port", json!(0)).expect_err("invalid port");
        assert!(matches!(err, SetPathError::InvalidValue { .. }));
        let updated =
            set_config_path(&config, "plugins.hello.city", json!("Berlin")).expect("create");
        assert_eq!(
            updated.plugins.get("hello"),
            Some(&json!({ "city": "Berlin" }))
        );
    }

    #[test]
    fn rejects_json_encoded_string_values_but_not_plain_strings() {
        let config = SmabarConfig::default();
        let err = set_config_path(&config, "plugins.hello.data", json!(r#"{"a": 1}"#))
            .expect_err("encoded object");
        assert!(matches!(err, SetPathError::JsonEncodedString { .. }));

        let updated = set_config_path(&config, "language", json!("de")).expect("plain string");
        assert_eq!(updated.language, "de");
    }

    #[test]
    fn appearance_tokens_are_validated_like_theme_files() {
        let config = SmabarConfig::default();

        let ok = set_config_path(
            &config,
            "appearance.tokens",
            json!({
                "--sb-bar-opacity": "55%",
                "--sb-accent": "#00ff88",
                "--sb-scale": "1.25"
            }),
        )
        .expect("valid tokens accepted");
        assert_eq!(ok.appearance.tokens.len(), 3);
        let unrelated = set_config_path(&ok, "language", json!("de"))
            .expect("a persisted scale must not block unrelated config writes");
        assert_eq!(
            unrelated
                .appearance
                .tokens
                .get("--sb-scale")
                .map(String::as_str),
            Some("1.25")
        );

        // Unsafe CSS, closed/internal smabar names, invalid ranges and
        // alpha-bearing shared surfaces all use the theme boundary rules.
        for bad in [
            json!({ "--sb-accent": "red; display: none" }),
            json!({ "--sb-accent": "url(javascript:alert(1))" }),
            json!({ "not-a-custom-property": "red" }),
            json!({ "--sb-accent": "" }),
            json!({ "--sb-not-real": "1rem" }),
            json!({ "--sb-autohide-edge-gap": "1rem" }),
            json!({ "--sb-bar-opacity": "101%" }),
            json!({ "--sb-scale": "0.5" }),
            json!({ "--sb-bar-bg": "rgba(10, 20, 30, .5)" }),
        ] {
            let err = set_config_path(&config, "appearance.tokens", bad.clone())
                .expect_err("invalid token rejected");
            assert!(
                matches!(err, SetPathError::InvalidToken { .. }),
                "{bad} gave {err:?}"
            );
        }

        // Single-key writes go through the same gate.
        let err = set_config_path(&config, "appearance.tokens.--sb-accent", json!("a; b"))
            .expect_err("invalid single token rejected");
        assert!(matches!(err, SetPathError::InvalidToken { .. }));

        let extension = set_config_path(
            &config,
            "appearance.tokens.--plugin-card-glow",
            json!("0 0 8px red"),
        )
        .expect("extension namespace remains open");
        assert_eq!(
            extension
                .appearance
                .tokens
                .get("--plugin-card-glow")
                .map(String::as_str),
            Some("0 0 8px red")
        );

        let google = set_config_path(
            &config,
            "appearance.tokens",
            json!({"--sb-font-sans":"Wrong","--sb-font-sans-source":"google:roboto"}),
        )
        .expect("known Google font");
        assert_eq!(
            google.appearance.tokens["--sb-font-sans"],
            "'Roboto', system-ui, sans-serif"
        );
    }

    #[test]
    fn theme_activation_can_clear_an_invalid_stale_override() {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = crate::config::SmabarPaths::new(dir.path().join("smabar"));
        let mut config = SmabarConfig::default();
        config.appearance.tokens.insert(
            "--sb-bar-bg".to_string(),
            "rgba(10, 20, 30, .5)".to_string(),
        );

        let (updated, errors) =
            set_config_path_activating(&paths, &config, "theme", json!("paper"))
                .expect("theme switch clears the stale layer first");
        assert!(errors.is_empty());
        assert_eq!(updated.theme, "paper");
        assert!(updated.appearance.tokens.is_empty());
    }

    #[test]
    fn get_config_path_walks_dotted_paths() {
        let root = json!({ "layout": { "position": "top" }, "mcp": { "port": 7627 } });
        assert_eq!(get_config_path(&root, "mcp.port"), Some(&json!(7627)));
        assert_eq!(
            get_config_path(&root, "layout.position"),
            Some(&json!("top"))
        );
        assert_eq!(get_config_path(&root, "layout.missing"), None);
    }
}

#[cfg(test)]
#[path = "update_theme_tests.rs"]
mod theme_tests;
