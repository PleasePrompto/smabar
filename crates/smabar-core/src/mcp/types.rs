//! Parameter and output types of the MCP tools. Doc comments become schema
//! descriptions for the connected agent.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{
    AppearanceConfig, BarPosition, EffectsConfig, LayoutConfig, McpConfig, PopupsConfig,
    SettingsWindowConfig, ShortcutsConfig, SpecialShortcut, ZOrder,
};
use crate::fonts::{FontOption, FontSource};
// The plugin-domain types live next door; re-exported so `types` stays the
// one import site for the tool modules.
pub use super::plugin_types::PluginInfoOut;
use crate::providers::ProviderEvent;
use crate::themes::ThemeInfo;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BarSetPositionParams {
    /// Screen edge to occupy.
    pub position: BarPosition,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SettingsGetParams {
    /// Dotted path into the config, e.g. `layout.position` or
    /// `plugins.hello.city`; omit for the whole config.
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SettingsSetParams {
    /// Dotted path; the first segment must be one of zOrder, language,
    /// theme, pluginOrder, plugins, mcp, rendering, layout, shortcuts,
    /// pluginsHidden, pluginsDeactivated, effects, appearance, popups,
    /// settingsWindow, audio.
    pub path: String,
    /// New JSON value for that path. A real JSON value — never a
    /// JSON-encoded string.
    #[schemars(schema_with = "any_json_value_schema")]
    pub value: Value,
}

/// Explicit schema for [`SettingsSetParams::value`]: a bare
/// `serde_json::Value` derives the empty schema `{}`, and some MCP clients
/// then serialize objects as JSON-encoded strings. Naming every JSON type
/// steers them to pass the value itself.
pub(super) fn any_json_value_schema(
    _generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    schemars::json_schema!({
        "type": ["object", "array", "string", "number", "boolean", "null"],
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FontListParams {
    /// Case-insensitive substring matched against the family or catalog id;
    /// omit to list the most popular Google fonts or the system inventory.
    pub query: Option<String>,
    /// Restrict results to `system` or `google`; omit to search both.
    pub source: Option<FontSource>,
    /// `true` returns only monospace families; `false` excludes them; omit
    /// for both.
    pub monospaced: Option<bool>,
    /// Maximum result count. Defaults to 50 and is always capped at 100.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeGetParams {
    /// Theme name (`[a-z0-9-]`), as listed by `theme_list`; omit for the
    /// active theme.
    pub name: Option<String>,
    /// JSON Pointer into the full contract, e.g. `/fonts` or
    /// `/baseTokens/--sb-accent`. Omit for the compact authoring overview.
    /// Large nodes return a child-path index instead of their full contents.
    pub contract_path: Option<String>,
    /// Offset into a large node's child-path index (100 entries per page).
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ThemeRemoveParams {
    /// Drop-in theme name from `theme_list`. Activate another theme before
    /// removing the active one. Bundled themes cannot be removed.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ThemeWriteParams {
    /// Theme name (`[a-z0-9-]`); bundled names (default, paper, terminal,
    /// topbar) are read-only.
    pub name: String,
    /// Flat JSON object mapping CSS custom-property names (`--sb-accent`,
    /// `--my-plugin-glow`, …) to simple CSS values. Must be a real JSON
    /// object, not a JSON string. Tokens missing here fall back to the
    /// default theme at resolve time.
    #[schemars(schema_with = "string_map_schema")]
    pub tokens: Value,
    /// Optional behavior block: a JSON object of dotted config paths applied
    /// ONE-SHOT each time the theme is activated (allowed: layout.*,
    /// appearance chrome/alignment/pluginAccent, effects.*, shortcuts
    /// labels/sizes, zOrder, popups.position; never appearance.tokens).
    /// Example: {"layout.width": "auto",
    /// "appearance.tileChrome": "flat"}. Omit for a pure look theme.
    #[schemars(schema_with = "json_object_schema")]
    pub settings: Option<Value>,
    /// Optional self-describing metadata: a JSON object with the string
    /// fields `name` (display name), `author`, `version`, `description`
    /// (each 1-200 characters). Ignored by the resolver, preserved on
    /// export — fill it when creating a theme meant to be shared.
    #[schemars(schema_with = "json_object_schema")]
    pub meta: Option<Value>,
}

fn string_map_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": { "type": "string" },
    })
}

fn json_object_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "object" })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginOrderSetParams {
    /// Tile ids in display order. Every tile comes from a plugin:
    /// `plugin:<pluginId>:<tileId>`.
    pub order: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginSetActiveParams {
    /// Plugin id (`[a-z0-9-]`), as listed by `plugin_list`.
    pub id: String,
    /// `false` stops the process and keeps it stopped across restarts;
    /// `true` starts it again. Nothing is deleted either way.
    pub active: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetVisibleParams {
    /// Tile id, always `plugin:<pluginId>:<tileId>`.
    pub tile_id: String,
    /// `false` takes the tile off the bar while its plugin keeps running;
    /// `true` shows it again.
    pub visible: bool,
}

/// Confirmation of a completed mutation.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AckResult {
    pub message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BarStateResult {
    pub layout: LayoutConfig,
    pub z_order: ZOrder,
    pub language: String,
    pub theme: String,
    pub plugin_order: Vec<String>,
    /// Tile ids taken off the bar; their plugins keep running.
    pub plugins_hidden: Vec<String>,
    /// Plugin ids switched off: no process runs for them.
    pub plugins_deactivated: Vec<String>,
    pub shortcuts: ShortcutsConfig,
    pub effects: EffectsConfig,
    pub appearance: AppearanceConfig,
    pub popups: PopupsConfig,
    pub mcp: McpConfig,
    pub settings_window: SettingsWindowConfig,
    pub plugins: Vec<PluginInfoOut>,
}

/// Selects one pinned shortcut by its pin id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShortcutIdParams {
    /// Generated pin id, as listed by `shortcut_list`.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppSearchParams {
    /// Case-insensitive substring matched against application names and
    /// comments; an empty string lists every installed application.
    pub query: String,
}

/// One installed application (no icon data — the shell fetches icons).
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoOut {
    /// XDG desktop-file id, usable as `shortcut_add` source on Linux.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_id: Option<String>,
    /// Absolute native application/link path, usable as `shortcut_add` source on Windows/macOS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppSearchResult {
    pub apps: Vec<AppInfoOut>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutAddParams {
    /// XDG desktop-file id from `app_search` (exactly one source; omit for a separator).
    pub desktop_id: Option<String>,
    /// Absolute path to an existing file or folder (exactly one source).
    pub path: Option<String>,
    /// http(s) website URL, e.g. "https://www.bild.de/" (exactly one source).
    pub url: Option<String>,
    /// Curated system item (`computer` or `trash`; exactly one source).
    pub special: Option<SpecialShortcut>,
    /// Display label; defaults to the app/path/system-item name or website host.
    pub label: Option<String>,
    /// Insert position among the pinned shortcuts; appends when omitted.
    pub index: Option<usize>,
    /// Add a visual separator instead of a launchable shortcut.
    #[serde(default)]
    pub separator: bool,
}

/// One pinned shortcut, resolved for listing. Icon bytes are deliberately
/// omitted (they are large base64 blobs); `icon_resolved` tells whether the
/// shell will show a real icon or an initial-letter tile.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinnedShortcutOut {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_id: Option<String>,
    /// Set for an absolute file/folder path pin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Set for website pins: the URL that opens in the system browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Set for curated system-item pins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special: Option<SpecialShortcut>,
    pub icon_resolved: bool,
    pub separator: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutListResult {
    pub pinned: Vec<PinnedShortcutOut>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeListResult {
    pub themes: Vec<ThemeInfo>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontListResult {
    /// Matching system families and Google catalog entries. For Google
    /// themes, pair `cssStack` with the source token `google:<catalog-id>`.
    pub fonts: Vec<FontOption>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeGetResult {
    pub name: String,
    /// Fully resolved token map. A same-named drop-in patches its bundled
    /// base; a drop-in-only theme inherits the bundled default.
    pub tokens: crate::themes::ThemeMap,
    /// Behavior settings the theme applies on each activation. Every bundled
    /// base is complete; a custom drop-in may keep this empty or partial.
    pub settings: crate::themes::ThemeSettings,
    /// Bundled theme providing inherited values (`default` for a drop-in-only
    /// theme, or the matching bundle for a same-named patch).
    pub base_theme: String,
    /// Self-describing metadata of a drop-in file, when it carries any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<crate::themes::ThemeMeta>,
    /// Compact authoring overview, requested contract fragment, or a paged
    /// child-path index when the requested fragment exceeds 16 KiB.
    pub contract: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsGetResult {
    /// The queried path; absent when the whole config was returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub value: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshotResult {
    pub providers: Vec<ProviderEvent>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UiKitParams {
    /// One to four sections to return in full: `bestPractices`,
    /// `coverLayouts`, `snippets`, `behaviour`, `conventions`, `sanitizer`,
    /// `media`, `formContract`, `branding`, `renderTargets`, `tileChrome`,
    /// `charts`, `icons`, `tokens`, `classes` (every class), `all`, or a
    /// class category: `actions`, `forms`, `data`, `content`, `layout`,
    /// `overlays`, `feedback`, `disclosure`. Omit both parameters for the
    /// index; omit this one when using `classes`.
    #[schemars(length(min = 1, max = 4))]
    pub sections: Option<Vec<String>>,
    /// Exact class names to return, e.g. `["sb-section", "sb-table"]`.
    /// Omit `sections` when using this targeted lookup; unknown names fail
    /// instead of silently returning an incomplete result.
    pub classes: Option<Vec<String>>,
}

/// The plugin UI kit contract; sections outside the requested set are
/// omitted.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiKitResult {
    /// Contract version.
    pub version: u64,
    /// The map of the contract: one line per section and category plus the
    /// snippet names. Only in the parameterless index reply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Value>,
    /// Shared design rules for compact plugin panels, including ~340px flyouts.
    /// Always included, whatever section was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_rules: Option<Value>,
    /// Practical design workflow and review checklist. Included with `all`
    /// and when `bestPractices` is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_practices: Option<Value>,
    /// Live-resolved tokens of the ACTIVE theme (`--sb-*` custom properties).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<crate::themes::ThemeMap>,
    /// Where the machine-readable token/theme/config contract lives
    /// (theme_get). A pointer, included with the `tokens` section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_contract: Option<Value>,
    /// The sb-* kit classes with purpose, example markup and category:
    /// smabar's own core vocabulary for `all`, every class for `classes`,
    /// one category's slice for a category section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classes: Option<Value>,
    /// What each class category holds and how to fetch it — the index to
    /// the classes `all` does not include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_categories: Option<Value>,
    /// The markup hooks the shell implements. `all` carries the index (one
    /// line per hook); `behaviour` carries the full entries with examples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behaviour: Option<Value>,
    /// The nine named tile cover recipes. Every reply carries the index
    /// (one line per layout); `coverLayouts` carries the full entries with
    /// markup, animation and sizing notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_layouts: Option<Value>,
    /// Icon names usable via `data-lucide`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons_usage: Option<Value>,
    /// The `data-chart` conventions (donut, sparkline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charts: Option<Value>,
    /// What the HTML sanitizer keeps and drops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitizer: Option<Value>,
    /// Images, videos, links: allowed URLs/attributes and media classes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Value>,
    /// How `data-field` form values reach the plugin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_contract: Option<Value>,
    /// Per-tile accent branding via the manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branding: Option<Value>,
    /// Shell-enhanced data attributes (`data-marquee`, `data-badge`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conventions: Option<Value>,
    /// Valid `ui.render` targets including proactive popups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_targets: Option<Value>,
    /// Global and per-tile tile chrome rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_chrome: Option<Value>,
    /// How to keep a bar tile a stable, hugging width, and the height budget
    /// it has. Always included: `designRules` points at it, and it is the
    /// most concrete layout guidance in the contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_sizing: Option<Value>,
    /// Copy-paste flyout skeletons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippets: Option<Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BarScreenshotParams {
    /// What to shoot. Omit for the whole bar. A namespaced tile id from
    /// `bar_get_state` (`plugin:<pluginId>:<tileId>`), `shortcut:<id>`, or
    /// an open surface: `flyout`, `overlay` (secondary solo row), `settings`, `popup`.
    pub target: Option<String>,
    /// Magnification, 1-4. Small tiles are easier to judge at 2-3.
    pub scale: Option<u8>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BarUiStateParams {
    /// `open_flyout` (needs `tileId`), `close_flyout`, `open_overlay`,
    /// `close_overlay`, `open_settings`, `close_settings`.
    pub action: crate::capture::UiAction,
    /// The tile whose flyout to open, e.g. `plugin:weather:current`.
    pub tile_id: Option<String>,
    /// For `open_settings`: the group to show — `bar`, `design`, `shortcuts`,
    /// `plugins`, `system`, `system/updates`, `legal`, or a page: `plugins/store` (Community
    /// Store), `design/themes` (Community Themes). Default `bar`.
    pub group: Option<String>,
}
