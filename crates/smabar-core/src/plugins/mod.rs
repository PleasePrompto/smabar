//! Plugin supervisor: out-of-process plugins speaking JSON-RPC 2.0 as NDJSON
//! over stdin/stdout.
//!
//! [`PluginSupervisor::start`] scans [`crate::config::SmabarPaths::plugins_dir`]
//! for folders containing a `smabar.json` manifest, spawns each plugin as a
//! child process (`uv run --script` for the python runtime, a literal command
//! for exec), supervises it (initialize handshake, periodic ping, exponential
//! restart backoff), hot-reloads on folder changes, and fans plugin output out
//! as [`PluginEvent`]s. Every stdout line that is not valid JSON-RPC and all
//! stderr output are captured into `logs/plugin-<id>.log`, so plain `print()`
//! debugging inside a plugin lands in its log file for free.
//!
//! A plugin can be switched off ([`activation`]) or deleted ([`remove`]).
//! Those are two different things and neither is "hiding a tile", which is
//! purely a shell-side reading of `pluginsHidden` and never reaches here —
//! a hidden tile's plugin keeps running on purpose.

mod activation;
mod await_status;
mod backoff;
#[cfg(test)]
mod command_tests;
mod commands;
mod handlers;
mod host;
mod icon;
mod lifecycle_guard;
mod logfile;
mod manifest;
mod process;
mod protocol;
mod provision;
mod remove;
mod replace;
mod rpc;
mod runner;
mod seed;
mod status;
pub(crate) mod stderr_cause;
mod supervisor;
mod watcher;

#[cfg(test)]
mod activation_tests;
#[cfg(test)]
mod dir_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod malformed_input_tests;
#[cfg(all(test, unix))]
mod provision_tests;
#[cfg(test)]
mod remove_tests;
#[cfg(test)]
mod replace_tests;
#[cfg(test)]
mod seed_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

pub use activation::set_plugin_active;
pub use await_status::{ReloadOutcome, StatusWatcher};
pub use commands::PluginCommandInfo;
pub use host::{HostPort, HostRequest, HostSession};
pub use logfile::{LOG_SOURCES, append_plugin_log};
pub use manifest::{
    MANIFEST_FILE, ManifestError, PluginManifest, PluginRuntime, PluginTileDef, TileScale,
    is_valid_plugin_id,
};
pub use provision::{PYTHON_VERSION, RuntimeFailureKind, RuntimeProvisioner, RuntimeStatus};
pub use remove::{PluginRemoval, RemoveError, remove_plugin};
pub use replace::{ReplaceError, ReplaceOutcome};
pub(crate) use seed::is_build_artifact;
pub use seed::{plugins_with_pending_update, seed_bundled_plugins, sweep_orphaned_data};
pub use status::{PluginInfo, UiSnapshot};
pub use supervisor::PluginSupervisor;
pub(crate) use supervisor::manifest_start_error;

/// Host-provided knobs. The app layer reads the environment and fills this
/// in; the core reads no config env vars itself (child processes still
/// inherit `PATH`, and `process.rs` prepends `PYTHONPATH` for the SDK).
#[derive(Debug, Clone, Default)]
pub struct SupervisorOptions {
    /// Desktop-only services; absent in headless/core-only hosts.
    pub host: Option<HostPort>,
    /// Directory prepended to python plugins' `PYTHONPATH` (the bundled SDK).
    pub sdk_path: Option<PathBuf>,
    /// Explicit `uv` binary; when unset, `uv` is looked up on `PATH`.
    pub uv_override: Option<PathBuf>,
    /// Directory where uv installs managed Python distributions.
    pub python_install_dir: Option<PathBuf>,
}

impl SupervisorOptions {
    pub fn capabilities(&self) -> &'static [&'static str] {
        if self.host.is_some() {
            &["commands", "popups", "audio"]
        } else {
            &["commands"]
        }
    }
}

/// Errors returned by [`PluginSupervisor`] entry points.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("{message}")]
    Command { message: String },
    /// No plugin with this id is registered (never scanned, or removed).
    #[error("no plugin with id \"{id}\" is registered")]
    UnknownPlugin { id: String },
    /// The plugin is registered but its supervision task has ended (final
    /// failure or shutdown), so it cannot receive messages.
    #[error("plugin \"{id}\" is not running")]
    NotRunning { id: String },
    /// The plugin's bounded command queue is full or it stopped consuming it.
    #[error("plugin \"{id}\" is busy and did not accept the action; try again")]
    Busy { id: String },
    /// The plugin exists, but the addressed tile is not part of its manifest.
    #[error(
        "plugin \"{id}\" has no tile \"{tile_id}\"; declared tiles: {}",
        available.join(", ")
    )]
    UnknownTile {
        id: String,
        tile_id: String,
        available: Vec<String>,
    },
    /// The plugin is installed but switched off, so starting it would
    /// contradict the user's own setting.
    #[error(
        "plugin \"{id}\" is deactivated: remove it from \"pluginsDeactivated\" in config.json to run it again"
    )]
    Deactivated { id: String },
}

/// Lifecycle state of a supervised plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    Starting,
    Running,
    Failed,
    Stopped,
    /// Switched off by the user (`pluginsDeactivated` in the config): no
    /// process runs, the installation is untouched, and it stays this way
    /// across restarts until the id is removed from that list. Distinct from
    /// [`PluginStatus::Stopped`], which is a plugin that WAS running and
    /// ended.
    Deactivated,
}

/// Events fanned out to shell/MCP subscribers via
/// [`PluginSupervisor::subscribe_events`].
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PluginEvent {
    /// A plugin with a valid manifest was registered. Re-emitted after a
    /// folder-change restart (the manifest may have changed).
    Added {
        plugin_id: String,
        name: String,
        /// Normalized local plugin icon; omitted when no usable icon exists.
        #[serde(skip_serializing_if = "Option::is_none")]
        icon_data_url: Option<String>,
        tiles: Vec<PluginTileDef>,
        /// The manifest's `settingsSchema`, so the settings panel can render
        /// a form for it; absent for plugins that declare none.
        #[serde(skip_serializing_if = "Option::is_none")]
        settings_schema: Option<serde_json::Value>,
    },
    /// The plugin's folder disappeared; its tiles should be removed.
    Removed { plugin_id: String },
    /// Lifecycle status change; `error` is set when `status` is `failed`.
    Status {
        plugin_id: String,
        status: PluginStatus,
        error: Option<String>,
    },
    /// The plugin rendered HTML for one of its tiles.
    UiRender {
        plugin_id: String,
        tile_id: String,
        target: String,
        html: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl_ms: Option<u32>,
    },
}
