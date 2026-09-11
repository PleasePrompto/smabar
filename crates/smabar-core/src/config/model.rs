//! Config schema plus load/save with atomic writes.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ConfigValidationError;
use super::SmabarPaths;

/// Errors from reading, writing, or watching the smabar config.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Reading or writing a config path failed.
    #[error("failed to access config path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The config file exists but is not valid JSON. It is never overwritten
    /// automatically — fix it by hand or delete it to restore defaults.
    #[error(
        "config file {path} contains invalid JSON (fix it by hand or delete it to restore defaults): {source}"
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// Serializing the config to JSON failed.
    #[error("failed to serialize config: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
    /// The JSON shape is valid but a value is outside the supported contract.
    #[error("config contains an invalid value: {source}")]
    Validation {
        #[from]
        source: ConfigValidationError,
    },
    /// The filesystem watcher could not be created or attached.
    #[error("config file watcher error: {source}")]
    Watch {
        #[from]
        source: notify::Error,
    },
    /// [`super::ConfigWatcher::spawn`] was called outside a tokio runtime.
    #[error("ConfigWatcher::spawn requires a running tokio runtime")]
    NoTokioRuntime,
}

/// Stacking behaviour of the bar window relative to other windows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ZOrder {
    /// Bar stays above all windows (default).
    #[default]
    Top,
    /// Bar lies behind all windows, below ordinary windows.
    Bottom,
}

/// Settings of the embedded MCP server. Changing them requires an app
/// restart; the server binds once at startup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct McpConfig {
    /// Serve the MCP endpoint (`http://127.0.0.1:<port>/mcp`) at startup.
    pub enabled: bool,
    /// TCP port on 127.0.0.1. The default 7627 spells "SMAR" on a phone keypad.
    pub port: u16,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 7627,
        }
    }
}

/// How the Linux webview is rendered. The choice is read before GTK starts,
/// so changes take effect after an app restart. Other platforms ignore it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum RenderingMode {
    /// Use native graphics when a DRM render node exists, otherwise Mesa
    /// software rendering.
    #[default]
    Auto,
    /// Leave WebKit's renderer selection untouched.
    Native,
    /// Force Mesa CPU rendering even when a GPU exists.
    Software,
}

/// Last user-selected geometry of the settings window, in logical (CSS)
/// pixels. The window is placed at `x`/`y` only while that spot is still
/// reachable on the active monitor; otherwise, and whenever they are `None`,
/// it opens centered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct SettingsWindowConfig {
    /// Window width in CSS pixels.
    pub width: u32,
    /// Window height in CSS pixels.
    pub height: u32,
    /// Screen x of the window's top-left corner, in CSS pixels.
    pub x: Option<i32>,
    /// Screen y of the window's top-left corner, in CSS pixels.
    pub y: Option<i32>,
}

impl Default for SettingsWindowConfig {
    fn default() -> Self {
        Self {
            width: 960,
            height: 680,
            x: None,
            y: None,
        }
    }
}

/// Persistent user configuration, stored at [`SmabarPaths::config_file`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct SmabarConfig {
    /// Bar layout: position (top|bottom), zone variant, and zone sizing.
    pub layout: super::LayoutConfig,
    /// Stacking: bar above everything ("top") or behind all windows ("bottom").
    pub z_order: ZOrder,
    /// UI language code, e.g. `"en"` or `"de"`.
    pub language: String,
    /// Active theme name: `"default"` (bundled) or the stem of a drop-in
    /// file in [`SmabarPaths::themes_dir`].
    pub theme: String,
    /// Where `export_theme` writes theme files. Empty (the default) means
    /// `themes_dir()/export`; otherwise an absolute path (`~/` expands).
    pub theme_export_dir: String,
    /// Global plugin-tile appearance.
    pub appearance: super::AppearanceConfig,
    /// Explicit tile order (tile ids, listed first); tiles not listed
    /// follow in registration order. Empty = natural registration order.
    pub plugin_order: Vec<String>,
    /// Tile ids hidden from the bar (they stay registered and orderable).
    /// This is HIDING: the plugin behind the tile keeps running and may
    /// still push popups.
    pub plugins_hidden: Vec<String>,
    /// Plugin ids that must not run. The supervisor stops them and never
    /// starts them again — not at boot and not when the folder watcher sees
    /// a write. Folder, data and settings are kept, so this is reversible.
    pub plugins_deactivated: Vec<String>,
    /// Pinned application shortcuts (dock zone).
    pub shortcuts: super::ShortcutsConfig,
    /// Visual effects (hover magnify and flyout peek).
    pub effects: super::EffectsConfig,
    /// Plugin popup notifications.
    pub popups: super::PopupsConfig,
    pub audio: super::AudioConfig,
    /// Embedded MCP server settings.
    pub mcp: McpConfig,
    /// The one-time default autostart attempt has been claimed. The OS owns
    /// the actual enabled state; this is not a settable user preference.
    pub autostart_initialized: bool,
    /// How the Linux webview renders (restart required).
    pub rendering: RenderingMode,
    /// Persisted settings-panel dimensions.
    pub settings_window: SettingsWindowConfig,
    /// Per-plugin settings, keyed by stable plugin id. The values belong to
    /// the plugins (free-form JSON); the core only persists them.
    pub plugins: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Default for SmabarConfig {
    fn default() -> Self {
        Self {
            layout: super::LayoutConfig::default(),
            z_order: ZOrder::default(),
            language: "en".to_string(),
            theme: "default".to_string(),
            theme_export_dir: String::new(),
            appearance: super::AppearanceConfig::default(),
            plugin_order: Vec::new(),
            plugins_hidden: Vec::new(),
            plugins_deactivated: Vec::new(),
            shortcuts: super::ShortcutsConfig::default(),
            effects: super::EffectsConfig::default(),
            popups: super::PopupsConfig::default(),
            audio: super::AudioConfig::default(),
            mcp: McpConfig::default(),
            autostart_initialized: false,
            rendering: RenderingMode::default(),
            settings_window: SettingsWindowConfig::default(),
            plugins: std::collections::BTreeMap::new(),
        }
    }
}

impl SmabarConfig {
    /// Load the config file. A missing file (or missing base directory) is
    /// created with defaults; invalid JSON is an error and the file is left
    /// untouched.
    pub fn load(paths: &SmabarPaths) -> Result<Self, ConfigError> {
        let file = paths.config_file();
        match fs::read_to_string(&file) {
            Ok(raw) => {
                let config: Self = serde_json::from_str(&raw)
                    .map_err(|source| ConfigError::Parse { path: file, source })?;
                config.validate()?;
                Ok(config)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save(paths)?;
                Ok(config)
            }
            Err(source) => Err(ConfigError::Io { path: file, source }),
        }
    }

    /// Atomically write the config as pretty-printed JSON (temp file + rename).
    pub fn save(&self, paths: &SmabarPaths) -> Result<(), ConfigError> {
        write_atomic(paths, &self.to_pretty_json()?)
    }

    /// Pretty-printed JSON with a trailing newline — the exact on-disk format.
    pub(super) fn to_pretty_json(&self) -> Result<String, ConfigError> {
        self.validate()?;
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|source| ConfigError::Serialize { source })?;
        json.push('\n');
        Ok(json)
    }

    /// Reject values consumers would otherwise clamp or silently ignore.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        super::validation::validate(self)
    }
}

/// Write via temp file + rename so readers never observe a half-written config.
pub(super) fn write_atomic(paths: &SmabarPaths, content: &str) -> Result<(), ConfigError> {
    fs::create_dir_all(paths.base_dir()).map_err(|source| ConfigError::Io {
        path: paths.base_dir().to_path_buf(),
        source,
    })?;
    let target = paths.config_file();
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, content).map_err(|source| ConfigError::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, &target).map_err(|source| ConfigError::Io {
        path: target,
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{
        AppearanceConfig, BarPosition, BarVariant, BarWidth, EffectsConfig, HoverMagnify,
        HoverPeek, LabelMode, LayoutBehavior, LayoutConfig, MonitorPreference, PopupPosition,
        PopupsConfig, ShortcutEntry, ShortcutsConfig, TileChrome, ZoneKind,
    };
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        (dir, paths)
    }

    #[test]
    fn load_creates_default_file_when_missing() {
        let (_dir, paths) = temp_paths();
        let config = SmabarConfig::load(&paths).expect("load");
        assert_eq!(config, SmabarConfig::default());
        assert_eq!(config.layout, LayoutConfig::default());
        assert_eq!(config.language, "en");
        assert_eq!(config.settings_window, SettingsWindowConfig::default());
        assert!(!config.autostart_initialized);

        let on_disk = fs::read_to_string(paths.config_file()).expect("config file written");
        assert!(on_disk.contains("\"position\": \"bottom\""));
        assert!(on_disk.contains("\"variant\": \"split\""));
        assert!(on_disk.contains("\"language\": \"en\""));
        assert!(on_disk.contains("\"theme\": \"default\""));
        assert!(on_disk.contains("\"rendering\": \"auto\""));
        assert!(!on_disk.contains("\"mode\""));
        assert!(on_disk.ends_with('\n'));
    }

    #[test]
    fn save_load_roundtrip_preserves_all_fields() {
        let (_dir, paths) = temp_paths();
        let config = SmabarConfig {
            layout: LayoutConfig {
                monitor: Some(MonitorPreference {
                    id: "linux:display-id".to_string(),
                    label: "Desk display".to_string(),
                }),
                position: BarPosition::Top,
                variant: BarVariant::Rows,
                divider_ratio: 0.5,
                primary_zone: ZoneKind::Shortcuts,
                width: BarWidth::Auto,
                max_width: 1_200,
                margin: 16,
                behavior: LayoutBehavior::Autohide,
                yield_to_fullscreen: false,
            },
            language: "de".to_string(),
            theme: "neon".to_string(),
            appearance: AppearanceConfig {
                tile_chrome: TileChrome::Flat,
                ..AppearanceConfig::default()
            },
            plugin_order: vec![
                "plugin:clock:clock".to_string(),
                "plugin:hello:main".to_string(),
            ],
            plugins_hidden: vec!["plugin:crypto:main".to_string()],
            plugins_deactivated: vec!["weather".to_string()],
            shortcuts: ShortcutsConfig {
                pinned: vec![ShortcutEntry {
                    id: "sc-1234abcd".to_string(),
                    desktop_id: Some("firefox.desktop".to_string()),
                    path: None,
                    url: None,
                    special: None,
                    label: Some("Browser".to_string()),
                    separator: false,
                }],
                labels: LabelMode::Below,
                icon_size: 48,
                label_size: 14,
            },
            effects: EffectsConfig {
                hover_magnify: HoverMagnify {
                    enabled: false,
                    scale: 1.4,
                    neighbors: 3,
                },
                hover_peek: HoverPeek {
                    enabled: false,
                    delay_ms: 850,
                },
            },
            popups: PopupsConfig {
                enabled: false,
                position: PopupPosition::TopCenter,
            },
            mcp: McpConfig {
                enabled: false,
                port: 9000,
            },
            rendering: RenderingMode::Software,
            autostart_initialized: true,
            settings_window: SettingsWindowConfig {
                width: 1_000,
                height: 720,
                x: Some(120),
                y: Some(-80),
            },
            ..SmabarConfig::default()
        };
        config.save(&paths).expect("save");
        assert_eq!(SmabarConfig::load(&paths).expect("load"), config);
    }

    #[test]
    fn defaults_enable_mcp_on_port_7627_with_empty_plugin_order() {
        let config = SmabarConfig::default();
        assert!(config.mcp.enabled);
        assert_eq!(config.mcp.port, 7627);
        assert!(config.plugin_order.is_empty());

        let json = serde_json::to_value(&config).expect("serialize");
        assert_eq!(json["pluginOrder"], serde_json::json!([]));
        assert_eq!(json["pluginsDeactivated"], serde_json::json!([]));
        assert_eq!(
            json["mcp"],
            serde_json::json!({ "enabled": true, "port": 7627 })
        );
        assert_eq!(json["appearance"]["tileChrome"], "card");
        assert_eq!(json["popups"]["enabled"], true);
        assert_eq!(json["rendering"], "auto");
        assert_eq!(
            json["settingsWindow"],
            serde_json::json!({
                "width": 960,
                "height": 680,
                "x": null,
                "y": null,
            })
        );
        assert_eq!(json["effects"]["hoverPeek"]["delayMs"], 400);
        assert_eq!(json["layout"]["maxWidth"], 0);
        assert_eq!(json["layout"]["behavior"], "reserve");
    }

    #[test]
    fn load_reports_invalid_json_and_leaves_the_file_alone() {
        let (_dir, paths) = temp_paths();
        fs::create_dir_all(paths.base_dir()).expect("create base dir");
        let broken = "{ this is not json";
        fs::write(paths.config_file(), broken).expect("write broken config");

        let err = SmabarConfig::load(&paths).expect_err("broken JSON must error");
        assert!(matches!(err, ConfigError::Parse { .. }));
        let message = err.to_string();
        assert!(message.contains("config.json"));
        assert!(message.contains("delete it to restore defaults"));
        // The broken file must never be silently overwritten.
        assert_eq!(
            fs::read_to_string(paths.config_file()).expect("read back"),
            broken
        );
    }

    #[test]
    fn load_defaults_missing_fields_but_rejects_unknown_ones() {
        let (_dir, paths) = temp_paths();
        fs::create_dir_all(paths.base_dir()).expect("create base dir");
        fs::write(paths.config_file(), r#"{"language":"de"}"#).expect("write partial config");
        let partial = SmabarConfig::load(&paths).expect("load partial config");
        assert_eq!(
            partial,
            SmabarConfig {
                language: "de".to_string(),
                ..SmabarConfig::default()
            }
        );
        assert!(
            serde_json::from_value::<SmabarConfig>(serde_json::json!({
                "mode": "floating-pill-top"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SmabarConfig>(serde_json::json!({
                "shortcuts": {"showLabels": false}
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SmabarConfig>(serde_json::json!({
                "mcp": {"poort": 7627}
            }))
            .is_err()
        );

        let mut config = serde_json::to_value(SmabarConfig::default()).expect("serialize");
        config["futureField"] = serde_json::json!({"nested": true});
        fs::write(
            paths.config_file(),
            serde_json::to_string(&config).expect("render config"),
        )
        .expect("write unknown config");
        assert!(SmabarConfig::load(&paths).is_err());
    }

    #[test]
    fn load_rejects_semantically_invalid_values_without_rewriting_them() {
        let (_dir, paths) = temp_paths();
        fs::create_dir_all(paths.base_dir()).expect("create base dir");
        let invalid = r#"{"mcp":{"port":0}}"#;
        fs::write(paths.config_file(), invalid).expect("write invalid config");

        let error = SmabarConfig::load(&paths).expect_err("port zero must fail");

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains("mcp.port"));
        assert_eq!(
            fs::read_to_string(paths.config_file()).expect("read back"),
            invalid
        );
    }
}
