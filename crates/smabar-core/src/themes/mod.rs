//! Flat key-value theming: four complete bundled themes plus drop-in themes.
//!
//! Every `themes/*.json` from the repo is compiled into the binary; together
//! they are the source of truth for all design-token keys (`--sb-*` CSS
//! custom properties), and `default.json` additionally anchors every token's
//! fallback value. Additional themes are drop-in JSON files in
//! [`SmabarPaths::themes_dir`]; per-token overrides win, missing tokens fall
//! back to the bundled default. Smabar's `--sb-*` namespace is closed to the
//! documented base/component catalog; plugins may add tokens under a different
//! custom-property namespace. A drop-in sharing a bundled name patches
//! that bundled theme per-token; a broken drop-in never breaks the bar — it
//! logs a warning and falls back.
//!
//! Besides tokens, a theme may carry two reserved non-token keys:
//! [`SETTINGS_KEY`], an object of dotted config paths applied one-shot when
//! the theme is activated (see [`settings`]) — a theme then ships look AND
//! behavior in one file — and [`META_KEY`], optional self-describing
//! metadata the resolver ignores but export/import preserve.

pub mod contract;
pub mod io;
pub mod settings;

use std::collections::BTreeMap;
use std::fs;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::config::{AppearanceConfig, LayoutConfig, SmabarConfig, SmabarPaths};

/// Flat mapping of CSS custom-property names to their values.
pub type ThemeMap = BTreeMap<String, String>;

/// Dotted config paths (e.g. `layout.position`) mapped to their JSON values.
pub type ThemeSettings = BTreeMap<String, serde_json::Value>;

/// Reserved key in a theme drop-in that carries the behavior settings block
/// instead of a token. Never a valid token name (no `--` prefix).
pub const SETTINGS_KEY: &str = "settings";

/// Reserved key for optional self-describing theme metadata. The resolver
/// ignores it entirely; [`io`] validates and preserves it on export/import so
/// shared theme files stay self-describing (store-ready).
pub const META_KEY: &str = "meta";

/// Optional self-describing metadata of a theme file. `name` is a display
/// name — the theme's identity stays the file stem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ThemeMeta {
    /// True when no field is set (the document then omits the `meta` key).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

const BUNDLED_JSON: &[(&str, &str)] = &[
    ("default", include_str!("../../../../themes/default.json")),
    ("paper", include_str!("../../../../themes/paper.json")),
    ("terminal", include_str!("../../../../themes/terminal.json")),
    ("topbar", include_str!("../../../../themes/topbar.json")),
];

/// One parsed theme file: flat tokens plus the two reserved blocks. Public
/// because [`io`] and its callers (MCP tool, Tauri commands) build and
/// consume whole documents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeDocument {
    pub tokens: ThemeMap,
    pub settings: ThemeSettings,
    pub meta: ThemeMeta,
}

static BUNDLED: LazyLock<Vec<(&'static str, ThemeDocument)>> = LazyLock::new(|| {
    BUNDLED_JSON
        .iter()
        .filter_map(|(name, json)| parse_document(json, name, true).map(|theme| (*name, theme)))
        .collect()
});

/// The compiled-in theme with `name`, or `None` for drop-in-only names.
pub fn bundled(name: &str) -> Option<&'static ThemeMap> {
    BUNDLED
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, theme)| &theme.tokens)
}

/// Is `name` one of the compiled-in themes? Their files are read-only for
/// `theme_write`; users customize them via same-named per-token drop-ins or
/// their own variants.
pub fn is_bundled(name: &str) -> bool {
    bundled(name).is_some()
}

/// The bundled default theme — the source of truth for all token keys and
/// the base every non-bundled theme resolves over.
pub fn bundled_default() -> &'static ThemeMap {
    // Unreachable in a healthy build: a test asserts the default bundle parses.
    static EMPTY: LazyLock<ThemeMap> = LazyLock::new(ThemeMap::new);
    bundled("default").unwrap_or(&EMPTY)
}

/// Resolve the token map for `theme_name`: the bundled map for that name
/// (the bundled default for drop-in-only names), overlaid with the drop-in
/// `themes/<name>.json` if one exists. Missing or invalid drop-ins log a
/// warning and fall back to their base; invalid entries inside an otherwise
/// valid drop-in are skipped individually.
pub fn resolve(paths: &SmabarPaths, theme_name: &str) -> ThemeMap {
    let mut map = bundled(theme_name)
        .cloned()
        .unwrap_or_else(|| bundled_default().clone());
    if let Some(dropin) = read_dropin(paths, theme_name) {
        map.extend(dropin.tokens);
    }
    crate::fonts::canonicalize_theme_fonts(&mut map);
    map
}

/// The behavior settings block of a theme. Bundled settings are the base;
/// a same-named drop-in patches them. Drop-in-only themes apply only the
/// settings they explicitly declare, so selecting one does not reset
/// unrelated behavior.
pub fn settings_block(paths: &SmabarPaths, theme_name: &str) -> ThemeSettings {
    let mut settings = BUNDLED
        .iter()
        .find(|(known, _)| *known == theme_name)
        .map(|(_, theme)| theme.settings.clone())
        .unwrap_or_default();
    if let Some(dropin) = read_dropin(paths, theme_name) {
        settings.extend(dropin.settings);
    }
    settings
}

/// Parsed content of one theme drop-in file.
type DropIn = ThemeDocument;

/// Reads and parses `themes/<name>.json`. `None` (with a warning) for
/// invalid names, unreadable files, and broken JSON; a MISSING file is the
/// normal no-customization case and stays silent. Invalid tokens inside a
/// valid file are skipped individually.
fn read_dropin(paths: &SmabarPaths, theme_name: &str) -> Option<DropIn> {
    if !is_valid_theme_name(theme_name) {
        tracing::warn!(theme_name, "invalid theme name; falling back to default");
        return None;
    }
    let file = paths.themes_dir().join(format!("{theme_name}.json"));
    let raw = match fs::read_to_string(&file) {
        Ok(raw) => raw,
        // No user customization for this theme — the everyday path.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(
                %error,
                path = %file.display(),
                "theme drop-in not readable; falling back to default"
            );
            return None;
        }
    };
    parse_document(&raw, &file.display().to_string(), false)
}

/// Parses one bundled or drop-in theme document. Invalid entries are skipped
/// individually so one hand-edited value cannot blank the bar.
fn parse_document(raw: &str, source: &str, bundled: bool) -> Option<ThemeDocument> {
    let entries = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw) {
        Ok(entries) => entries,
        Err(error) => {
            if bundled {
                tracing::error!(%error, theme = source, "bundled theme file is invalid; theme unavailable");
            } else {
                tracing::warn!(%error, path = source, "theme drop-in is not a JSON object; falling back to default");
            }
            return None;
        }
    };
    let mut document = ThemeDocument::default();
    for (key, value) in entries {
        if key == SETTINGS_KEY {
            match value {
                serde_json::Value::Object(block) => {
                    document.settings = block.into_iter().collect();
                }
                _ => tracing::warn!(
                    source,
                    "theme \"settings\" must be a JSON object of dotted config paths; ignoring it"
                ),
            }
            continue;
        }
        if key == META_KEY {
            // Tolerant like everything else here: unknown meta fields are
            // ignored by serde, a malformed block never breaks the theme.
            match serde_json::from_value::<ThemeMeta>(value) {
                Ok(meta) => document.meta = meta,
                Err(error) => {
                    tracing::warn!(source, %error, "ignoring malformed theme \"meta\" block; fix or remove its fields, or import the file to see strict validation errors");
                }
            }
            continue;
        }
        match value {
            serde_json::Value::String(value) => match contract::validate_token(&key, &value) {
                Ok(()) => {
                    document.tokens.insert(key, value);
                }
                Err(reason) => tracing::warn!(key, source, reason, "skipping invalid theme token"),
            },
            _ => tracing::warn!(
                key,
                source,
                "skipping theme token whose value is not a string"
            ),
        }
    }
    Some(document)
}

/// Every compiled-in theme name plus every `*.json` drop-in in the themes
/// directory, sorted and deduplicated (a drop-in sharing a bundled name
/// appears once — it patches the bundled theme).
pub fn available_themes(paths: &SmabarPaths) -> Vec<String> {
    let mut themes: Vec<String> = BUNDLED
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    if let Ok(entries) = fs::read_dir(paths.themes_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
            {
                themes.push(stem.to_string());
            }
        }
    }
    themes.sort();
    themes.dedup();
    themes
}

/// Preview colors extracted from a resolved token map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub accent: String,
    pub accent_2: String,
    /// Main bar surface background.
    pub surface: String,
    pub text: String,
}

/// The resolved CSS family stack and its provisioning source for one slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFont {
    pub family: String,
    pub source: String,
}

/// Primary and monospace fonts resolved for a theme preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFonts {
    pub sans: ThemeFont,
    pub mono: ThemeFont,
}

/// Read-only result of activating a theme over the current configuration.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreview {
    pub layout: LayoutConfig,
    /// Resolved theme tokens, without the current look's per-token overrides.
    pub appearance: AppearanceConfig,
}

/// Shares the activation rules with installed and remote theme previews.
pub fn preview(current: &SmabarConfig, tokens: ThemeMap, block: &ThemeSettings) -> ThemePreview {
    let (mut candidate, warnings) = settings::apply_settings(current.clone(), block);
    for warning in warnings {
        tracing::warn!(warning, "ignored invalid theme setting in preview");
    }
    candidate.appearance.tokens = tokens;
    ThemePreview {
        layout: candidate.layout,
        appearance: candidate.appearance,
    }
}

/// One known theme with the colors its resolved token map produces. Shared
/// by the MCP `theme_list` tool and the Tauri `list_themes` command.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInfo {
    pub name: String,
    /// `bundled` (compiled in) or `dropin` (`~/.smabar/themes/<name>.json`).
    pub source: String,
    /// True when this is the currently configured theme.
    pub active: bool,
    pub colors: ThemeColors,
    pub fonts: ThemeFonts,
    pub preview: ThemePreview,
    /// Self-describing metadata of a drop-in file, when it carries any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ThemeMeta>,
}

/// Summarizes every known theme for pickers: names, sources, active flag,
/// and the preview colors each resolves to.
pub fn summaries(paths: &SmabarPaths, current: &SmabarConfig) -> Vec<ThemeInfo> {
    available_themes(paths)
        .into_iter()
        .map(|name| {
            let resolved = resolve(paths, &name);
            let token = |key: &str| resolved.get(key).cloned().unwrap_or_default();
            let source = if is_bundled(&name) {
                "bundled"
            } else {
                "dropin"
            };
            let active = name == current.theme;
            let meta = dropin_meta(paths, &name);
            ThemeInfo {
                meta,
                colors: ThemeColors {
                    accent: token("--sb-accent"),
                    accent_2: token("--sb-accent-2"),
                    surface: token("--sb-bar-bg"),
                    text: token("--sb-text"),
                },
                fonts: ThemeFonts {
                    sans: ThemeFont {
                        family: token("--sb-font-sans"),
                        source: token("--sb-font-sans-source"),
                    },
                    mono: ThemeFont {
                        family: token("--sb-font-mono"),
                        source: token("--sb-font-mono-source"),
                    },
                },
                preview: preview(current, resolved.clone(), &settings_block(paths, &name)),
                name,
                source: source.to_string(),
                active,
            }
        })
        .collect()
}

/// Metadata of the drop-in file for `name`: `None` when there is no drop-in
/// or it carries no meta block.
pub fn dropin_meta(paths: &SmabarPaths, name: &str) -> Option<ThemeMeta> {
    read_dropin(paths, name)
        .map(|dropin| dropin.meta)
        .filter(|meta| !meta.is_empty())
}

/// Theme names are plain file stems (`[a-z0-9-]`) — never paths.
pub fn is_valid_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a theme can be activated right now. A broken drop-in still counts:
/// its tolerant resolver logs the problem and falls back while the user fixes it.
pub fn is_available(paths: &SmabarPaths, name: &str) -> bool {
    is_bundled(name)
        || (is_valid_theme_name(name) && paths.themes_dir().join(format!("{name}.json")).is_file())
}

/// Token keys are CSS custom-property names: `--` plus `[A-Za-z0-9_-]`.
pub fn is_valid_token_key(key: &str) -> bool {
    key.len() > 2
        && key.starts_with("--")
        && key[2..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Token values must be simple CSS values: no control characters, `;`, `{`,
/// `}`, and no `url(...)` other than `data:` or `https:`. This protects the
/// integrity of the shell stylesheet — it is not a security boundary.
pub fn is_valid_token_value(value: &str) -> bool {
    if value.is_empty() || value.chars().any(|c| c.is_control()) {
        return false;
    }
    if value.contains([';', '{', '}']) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(pos) = rest.find("url(") {
        let after = &rest[pos + 4..];
        let arg = after.trim_start().trim_start_matches(['"', '\'']);
        if !(arg.starts_with("data:") || arg.starts_with("https:")) {
            return false;
        }
        rest = after;
    }
    true
}

#[cfg(test)]
mod io_tests;
#[cfg(test)]
mod tests;
