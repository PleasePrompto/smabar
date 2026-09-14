//! MCP parameter and result types for the plugin tools.
//!
//! Split out of `types.rs` so both files stay under the line limit; the
//! plugin domain (manifest, files, logs, reload outcomes, the authoring
//! guide) is the natural seam.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::logging::LogEntry;
use crate::plugins::{PluginInfo, PluginStatus, PluginTileDef};

/// Selects one plugin by its stable id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginIdParams {
    /// Plugin id (`[a-z0-9-]`), as listed by `plugin_list`.
    pub id: String,
}

/// One UI action to forward to a running plugin.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginActionParams {
    /// Plugin id (`[a-z0-9-]`), as listed by `plugin_list`.
    pub id: String,
    /// Manifest-local tile id, e.g. `weather`; do not use the shell's
    /// `plugin:<pluginId>:<tileId>` registry id.
    pub tile: String,
    /// Action name handled by `@app.on_action`, exactly as written in
    /// `data-action` or a context-menu entry.
    pub action: String,
    /// Optional real JSON payload delivered as the handler's `value`
    /// argument; never a JSON-encoded string.
    #[serde(default)]
    #[schemars(schema_with = "super::types::any_json_value_schema")]
    pub value: Option<Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginWriteFileParams {
    /// Plugin id (`[a-z0-9-]`); its folder is created when missing.
    pub id: String,
    /// File path relative to the plugin folder, e.g. `smabar.json` or
    /// `plugin.py`. Parent directories are created; `..` and absolute paths
    /// are rejected.
    pub path: String,
    /// Full new file content (UTF-8).
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PluginLogsParams {
    /// Plugin id to read `logs/plugin-<id>.log`; omit for the core smabar log.
    pub id: Option<String>,
    /// Minimum level: trace, debug, info, warn, or error.
    pub level: Option<String>,
    /// Keep only the newest N entries (default 100).
    pub limit: Option<usize>,
    /// Keep only entries whose raw JSONL line contains this substring.
    pub contains: Option<String>,
}

/// One known plugin: manifest data (when registered) plus lifecycle status.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfoOut {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: PluginStatus,
    /// Failure reason when status is `failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiles: Option<Vec<PluginTileDef>>,
    /// Absolute plugin folder path (missing for a folder that failed to load).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Absolute writable data directory (`~/.smabar/data/<id>/`).
    pub data_dir: String,
    /// True for a bundled plugin that was edited locally while a newer
    /// bundled version shipped: the replacement waits for a user decision.
    /// Always serialized: the derived schema lists it as required.
    pub update_available: bool,
    /// Tile ids of this plugin the user hid (`pluginsHidden`): the plugin
    /// runs, but these tiles are off the bar until `plugin_set_visible`.
    /// Always present, including an empty list, as required by the output schema.
    pub hidden_tiles: Vec<String>,
}

impl PluginInfoOut {
    pub fn from_info(
        info: PluginInfo,
        data_dir: String,
        update_available: bool,
        plugins_hidden: &[String],
    ) -> Self {
        let (name, version, description, tiles) = match info.manifest {
            Some(manifest) => (
                Some(manifest.name),
                Some(manifest.version),
                manifest.description,
                Some(manifest.tiles),
            ),
            None => (None, None, None, None),
        };
        let hidden_tiles = tiles
            .iter()
            .flatten()
            .filter(|tile| plugins_hidden.contains(&format!("plugin:{}:{}", info.id, tile.id)))
            .map(|tile| tile.id.clone())
            .collect();
        Self {
            id: info.id,
            name,
            version,
            description,
            status: info.status,
            error: info.error,
            tiles,
            dir: info.dir.map(|dir| dir.display().to_string()),
            data_dir,
            update_available,
            hidden_tiles,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginListResult {
    pub plugins: Vec<PluginInfoOut>,
}

/// Which plugin's data directory to read, and optionally which file.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDataParams {
    /// Plugin id (the folder name under `~/.smabar/plugins/`).
    pub id: String,
    /// File to read, relative to the data directory. Omit to list it.
    pub path: Option<String>,
}

/// One file in a plugin's data directory, listed without reading it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DataFileOut {
    /// Path relative to the data directory.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Last modification, seconds since the unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

/// A plugin's data directory: the listing, or one file's content.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDataResult {
    pub plugin_id: String,
    /// Absolute path of the data directory.
    pub dir: String,
    /// False when the plugin has never written anything.
    pub exists: bool,
    /// Bytes in the listing, or the size of the single file read.
    pub total_bytes: u64,
    /// True when the listing hit its entry cap.
    pub truncated_listing: bool,
    /// The listing; empty when a single file was requested.
    pub files: Vec<DataFileOut>,
    /// The requested file; absent for a listing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PluginFileOut>,
}

/// One file inside a plugin folder.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileOut {
    /// Path relative to the plugin folder.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// UTF-8 content; omitted for large or binary files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// True when the content was omitted because the file exceeds 64 KiB.
    pub truncated: bool,
    /// True when the content was omitted because the file is not UTF-8.
    pub binary: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginReadResult {
    /// Absolute plugin folder path.
    pub dir: String,
    pub files: Vec<PluginFileOut>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginWriteResult {
    /// Path of the written file relative to the plugin folder.
    pub path: String,
    pub bytes_written: usize,
    pub message: String,
    /// What the (re)start actually did. Absent when the write could not start
    /// anything yet — that is the normal answer for an entry script written
    /// before its manifest exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload: Option<ReloadOutcomeOut>,
}

/// What went wrong during one (re)start, condensed so it cannot be missed.
#[derive(Debug, Default, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReloadWarnings {
    /// Number of distinct problems this start produced; 0 is a clean start.
    pub count: usize,
    /// The problems, each prefixed with its source (`shell:`, `core:`,
    /// `stderr:`), newest last; at most ten.
    pub messages: Vec<String>,
}

/// The real outcome of a plugin (re)start.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReloadOutcomeOut {
    /// `running`, `failed`, `starting`, or `stopped`.
    pub status: PluginStatus,
    /// Why it failed, when `status` is `failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `false` when the wait timed out before the plugin settled; `status` is
    /// then the last known level, not a final answer.
    pub settled: bool,
    /// `false` when the plugins folder is not being watched — then only an
    /// explicit plugin_reload picks changes up.
    pub hot_reload: bool,
    /// Tile ids the plugin currently declares.
    pub tiles: Vec<String>,
    /// The newest log entries of this (re)start (up to 20).
    pub logs: Vec<PluginLogEntry>,
    /// Every warning this start produced: markup the shell removed or
    /// flagged, requests the core rejected, the stderr cause of a crash.
    /// `count` 0 is a clean start.
    pub warnings: ReloadWarnings,
    /// Declared tiles whose tile rendered during this start. A declared
    /// tile missing here has not pushed a tile yet.
    pub rendered: Vec<String>,
    /// Pending visual review with concrete tool calls for the declared tiles
    /// and the machine-checkable completion criteria under `done`. Only
    /// returned after a successful start; runtime success is not UI approval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_review: Option<Value>,
}

/// Result of an explicit plugin restart.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginReloadResult {
    pub message: String,
    pub reload: ReloadOutcomeOut,
}

/// One line of a plugin's JSONL log file.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginLogEntry {
    /// Unix timestamp in milliseconds.
    pub ts: u64,
    pub level: String,
    /// `log` (an app.log call), `stdout` (a non-RPC line), `stderr` (a
    /// traceback lands here), `core` (a request smabar rejected), or `shell`
    /// (the bar's report about the rendered markup).
    pub source: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginLogsResult {
    /// Entries from `logs/plugin-<id>.log`; present when `id` was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_entries: Option<Vec<PluginLogEntry>>,
    /// Entries from the core smabar log; present when no `id` was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_entries: Option<Vec<LogEntry>>,
    /// Lines that could not be parsed as log entries.
    pub skipped_lines: usize,
}

/// Which part of the plugin authoring guide to return.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuideParams {
    /// Omit for the START document (`start`). One reference block instead:
    /// `manifest` (incl. the generated JSON Schema), `sdk`, `storage`,
    /// `lifecycle`, `debugging`, `template` (a complete multi-file plugin in
    /// write order), `publishing` (plugins and themes), `capabilities` (the index alone), or `all`.
    pub section: Option<String>,
}

/// The plugin authoring contract: how to write, start and debug a plugin.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuideResult {
    /// Guide version.
    pub version: u64,
    /// The ordered steps from nothing to a finished plugin, each with its
    /// completion criterion and reference calls. Always included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub golden_path: Option<Value>,
    /// The words the steps and the tool replies use (surface, sibling, clean
    /// start, photograph, cover), defined once. Start and `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<Value>,
    /// The markup rules in brief: they decide what the HTML looks like, so
    /// they arrive before the API. Start and `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<Value>,
    /// What each `section` discloses. Start only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Value>,
    /// Host techniques and targeted follow-up calls, independent of installed
    /// plugins, with the live services/providers sent at initialize. Start,
    /// `capabilities` and `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Value>,
    /// Manifest fields, validation rules, and the python PEP 723 header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Value>,
    /// JSON Schema generated from the manifest struct the core parses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_schema: Option<Value>,
    /// The Python SDK surface, member by member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk: Option<Value>,
    /// Where and how a plugin persists things: the data directory, SQLite,
    /// atomic writes, serving cached data during a refresh, and networking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<Value>,
    /// Startup order, the shared handler lock, error and shutdown behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Value>,
    /// Where to look when a plugin misbehaves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debugging: Option<Value>,
    /// Optional public sharing and the existing GitHub/Community Store workflow
    /// for both plugins and themes. Included with `publishing` and `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publishing: Option<Value>,
    /// A complete, runnable multi-file plugin as `files` in write order, plus
    /// a `note` on copying it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<Value>,
}
