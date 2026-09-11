//! The `smabar.json` plugin manifest: schema, parsing, validation.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::config::TileChrome;

/// File name of the manifest inside a plugin folder.
pub const MANIFEST_FILE: &str = "smabar.json";
const MAX_ICON_SVG_BYTES: usize = 8 * 1024;

/// Errors from loading or validating a plugin manifest. They become the
/// plugin's `failed` status text, so every message must tell the author what
/// to fix.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The manifest file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The manifest file is not valid JSON or misses required fields.
    #[error("{path} is not a valid manifest: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// The manifest parsed but violates a semantic rule.
    #[error("invalid manifest {path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

/// How a plugin process is launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PluginRuntime {
    /// `uv run --script <entry>` — Python with PEP 723 inline dependencies.
    Python,
    /// The manifest's `command` array, verbatim.
    Exec,
}

/// Base text size of a two-line bar tile (`sb-tile-stack`), as a step rather
/// than a pixel value: the shell scales it against the height the bar
/// actually grants a tile, so the text follows the bar's size.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum TileScale {
    S,
    #[default]
    M,
    L,
}

/// One tile a plugin contributes to the bar.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", from = "RawPluginTileDef")]
#[schemars(!from)]
pub struct PluginTileDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub has_flyout: bool,
    /// Optional tile chrome override; the global appearance setting applies
    /// when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<TileChrome>,
    /// Optional base text size for a two-line tile; `m` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_scale: Option<TileScale>,
    /// Optional custom branding icon. The shell accepts exactly one SVG root
    /// and runs it through the plugin-markup sanitizer before rendering it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_svg: Option<String>,
    /// Prepend the plugin-folder icon to this cover. Defaults to false;
    /// a usable `iconSvg` takes precedence. Settings use the icon independently.
    #[serde(default)]
    pub use_plugin_icon: bool,
    /// Branding accent color for this tile (any simple CSS color value);
    /// overrides `--sb-accent` inside the plugin's tile and flyout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Secondary branding color (`--sb-accent-2`); falls back to `accent`.
    #[serde(default, rename = "accent2", skip_serializing_if = "Option::is_none")]
    pub accent_2: Option<String>,
    /// Text color on accent surfaces (`--sb-on-accent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_fg: Option<String>,
}

/// Parsed and validated `smabar.json`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", from = "RawPluginManifest")]
#[schemars(!from)]
pub struct PluginManifest {
    /// Stable plugin id, `[a-z0-9-]` — the key for settings, logs, events.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub protocol_version: u32,
    pub runtime: PluginRuntime,
    /// Script file relative to the plugin folder (python runtime).
    #[serde(default)]
    pub entry: Option<String>,
    /// Program plus arguments (exec runtime).
    #[serde(default)]
    pub command: Vec<String>,
    pub tiles: Vec<PluginTileDef>,
    /// JSON Schema of the plugin's settings, for MCP and the settings UI.
    #[serde(default)]
    pub settings_schema: Option<Value>,
    /// Resolved from the plugin folder, never accepted as manifest input.
    #[schemars(skip)]
    pub icon_data_url: Option<String>,
    #[schemars(skip)]
    diagnostics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifest {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: Option<Value>,
    protocol_version: u32,
    runtime: PluginRuntime,
    #[serde(default)]
    entry: Option<Value>,
    #[serde(default)]
    command: Option<Value>,
    tiles: Vec<Value>,
    #[serde(default)]
    settings_schema: Option<Value>,
    /// Listing metadata for the Community Store (keywords, os, minSmabar,
    /// external). The store validates it; the runtime only has to know the
    /// field exists so a listed plugin does not log an "unknown field"
    /// warning on every start.
    #[serde(default, rename = "store")]
    _store: Option<Value>,
    #[serde(flatten)]
    unknown: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginTileDef {
    id: String,
    name: String,
    #[serde(default)]
    has_flyout: Option<Value>,
    #[serde(default)]
    tile: Option<Value>,
    #[serde(default)]
    tile_scale: Option<Value>,
    #[serde(default)]
    icon_svg: Option<Value>,
    #[serde(default)]
    use_plugin_icon: Option<Value>,
    #[serde(default)]
    accent: Option<Value>,
    #[serde(default, rename = "accent2")]
    accent_2: Option<Value>,
    #[serde(default)]
    accent_fg: Option<Value>,
    #[serde(flatten)]
    unknown: BTreeMap<String, Value>,
}

impl From<RawPluginManifest> for PluginManifest {
    fn from(raw: RawPluginManifest) -> Self {
        let mut diagnostics = Vec::new();
        if !raw.unknown.is_empty() {
            diagnostics.push(format!(
                "unknown root fields {} were ignored; supported root fields are id, name, \
                 version, description, protocolVersion, runtime, entry, command, tiles, \
                 settingsSchema, and store",
                field_names(&raw.unknown)
            ));
        }
        let description =
            optional_value(raw.description, "description", "a string", &mut diagnostics);
        let entry = optional_value(raw.entry, "entry", "a string", &mut diagnostics);
        let command = optional_value(
            raw.command,
            "command",
            "an array of strings",
            &mut diagnostics,
        )
        .unwrap_or_default();
        let mut tiles: Vec<PluginTileDef> = raw
            .tiles
            .into_iter()
            .enumerate()
            .filter_map(|(index, value)| parse_tile(value, index, &mut diagnostics))
            .collect();
        let mut tile_ids = HashSet::new();
        tiles.retain(|tile| {
            if tile_ids.insert(tile.id.clone()) {
                return true;
            }
            diagnostics.push(format!(
                "the later tile with duplicate id {:?} was ignored; use unique tile ids",
                tile.id
            ));
            false
        });
        Self {
            id: raw.id,
            name: raw.name,
            version: raw.version,
            description,
            protocol_version: raw.protocol_version,
            runtime: raw.runtime,
            entry,
            command,
            tiles,
            settings_schema: raw.settings_schema,
            icon_data_url: None,
            diagnostics,
        }
    }
}

fn parse_tile(value: Value, index: usize, diagnostics: &mut Vec<String>) -> Option<PluginTileDef> {
    let raw = match serde_json::from_value::<RawPluginTileDef>(value) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!(
                "tiles[{index}] was ignored ({error}); use an object with string id and name"
            ));
            return None;
        }
    };
    Some(tile_from_raw(raw, index, diagnostics))
}

impl From<RawPluginTileDef> for PluginTileDef {
    fn from(raw: RawPluginTileDef) -> Self {
        tile_from_raw(raw, 0, &mut Vec::new())
    }
}

fn tile_from_raw(
    raw: RawPluginTileDef,
    index: usize,
    diagnostics: &mut Vec<String>,
) -> PluginTileDef {
    let path = format!("tiles[{index}] ({:?})", raw.id);
    if !raw.unknown.is_empty() {
        diagnostics.push(format!(
            "{path} unknown fields {} were ignored; supported tile fields are id, name, \
             hasFlyout, tile, tileScale, iconSvg, usePluginIcon, accent, accent2, and accentFg",
            field_names(&raw.unknown)
        ));
    }
    let mut tile = PluginTileDef {
        id: raw.id,
        name: raw.name,
        has_flyout: optional_value(
            raw.has_flyout,
            &format!("{path}.hasFlyout"),
            "true or false",
            diagnostics,
        )
        .unwrap_or_default(),
        tile: optional_value(
            raw.tile,
            &format!("{path}.tile"),
            "\"card\" or \"flat\"",
            diagnostics,
        ),
        tile_scale: optional_value(
            raw.tile_scale,
            &format!("{path}.tileScale"),
            "\"s\", \"m\", or \"l\"",
            diagnostics,
        ),
        icon_svg: optional_value(
            raw.icon_svg,
            &format!("{path}.iconSvg"),
            "a string containing one SVG root",
            diagnostics,
        ),
        use_plugin_icon: optional_value(
            raw.use_plugin_icon,
            &format!("{path}.usePluginIcon"),
            "true or false",
            diagnostics,
        )
        .unwrap_or_default(),
        accent: optional_value(
            raw.accent,
            &format!("{path}.accent"),
            "a CSS color string",
            diagnostics,
        ),
        accent_2: optional_value(
            raw.accent_2,
            &format!("{path}.accent2"),
            "a CSS color string",
            diagnostics,
        ),
        accent_fg: optional_value(
            raw.accent_fg,
            &format!("{path}.accentFg"),
            "a CSS color string",
            diagnostics,
        ),
    };
    if tile
        .icon_svg
        .as_ref()
        .is_some_and(|icon| icon.len() > MAX_ICON_SVG_BYTES)
    {
        tile.icon_svg = None;
        diagnostics.push(format!(
            "{path}.iconSvg was ignored; use at most {MAX_ICON_SVG_BYTES} UTF-8 bytes"
        ));
    }
    for (field, value) in [
        ("accent", &mut tile.accent),
        ("accent2", &mut tile.accent_2),
        ("accentFg", &mut tile.accent_fg),
    ] {
        if value
            .as_ref()
            .is_some_and(|value| value.len() > 128 || !crate::themes::is_valid_token_value(value))
        {
            *value = None;
            diagnostics.push(format!(
                "{path}.{field} was ignored; use a simple CSS color of at most 128 characters"
            ));
        }
    }
    tile
}

fn optional_value<T: DeserializeOwned>(
    value: Option<Value>,
    path: &str,
    expected: &str,
    diagnostics: &mut Vec<String>,
) -> Option<T> {
    let value = value?;
    match serde_json::from_value::<Option<T>>(value) {
        Ok(value) => value,
        Err(_) => {
            diagnostics.push(format!("{path} was ignored; use {expected}"));
            None
        }
    }
}

fn field_names(fields: &BTreeMap<String, Value>) -> String {
    fields.keys().cloned().collect::<Vec<_>>().join(", ")
}

/// A plugin id is a folder name, an env var value, a log file name and a
/// path segment — so the charset is deliberately narrow. One definition,
/// used by manifest validation, the MCP tools and the log writers alike.
pub fn is_valid_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl PluginManifest {
    /// Loads and validates `<dir>/smabar.json`.
    pub fn load(dir: &Path) -> Result<Self, ManifestError> {
        let path = dir.join(MANIFEST_FILE);
        let raw = std::fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.clone(),
            source,
        })?;
        let mut manifest = Self::parse(&raw, &path)?;
        manifest.icon_data_url = super::icon::load(dir, &mut manifest.diagnostics);
        Ok(manifest)
    }

    /// Parses and validates manifest content that is not on disk yet.
    ///
    /// `path` only labels errors. This is how a writer can reject a broken
    /// manifest BEFORE it lands in the plugins folder, instead of letting the
    /// watcher discover it and fail the plugin asynchronously.
    pub fn parse(raw: &str, path: &Path) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(raw).map_err(|source| ManifestError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        manifest.validate(path)?;
        Ok(manifest)
    }

    pub(crate) fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub(crate) fn format_diagnostic(items: &[&str]) -> String {
        format!(
            "smabar.json kept the plugin usable after ignoring: {}. Use plugin_guide and \
             its manifest schema for supported fields and values.",
            items.join("; ")
        )
    }

    #[cfg(test)]
    pub(crate) fn diagnostic(&self) -> Option<String> {
        (!self.diagnostics.is_empty()).then(|| {
            let items: Vec<_> = self.diagnostics.iter().map(String::as_str).collect();
            Self::format_diagnostic(&items)
        })
    }

    fn validate(&self, path: &Path) -> Result<(), ManifestError> {
        let invalid = |reason: String| ManifestError::Invalid {
            path: path.to_path_buf(),
            reason,
        };
        if !is_valid_plugin_id(&self.id) {
            return Err(invalid(format!(
                "id \"{}\" must be non-empty and contain only [a-z0-9-]",
                self.id
            )));
        }
        if self.protocol_version != 1 {
            return Err(invalid(format!(
                "protocolVersion {} is not supported (this smabar speaks version 1)",
                self.protocol_version
            )));
        }
        if self.tiles.is_empty() {
            return Err(invalid(
                "\"tiles\" must contain at least one tile".to_string(),
            ));
        }
        match self.runtime {
            PluginRuntime::Python if self.entry.as_deref().is_none_or(str::is_empty) => Err(
                invalid("runtime \"python\" requires an \"entry\" script file".to_string()),
            ),
            PluginRuntime::Exec if self.command.is_empty() => Err(invalid(
                "runtime \"exec\" requires a non-empty \"command\" array".to_string(),
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
