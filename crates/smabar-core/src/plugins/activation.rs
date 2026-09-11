//! Deactivation: plugins the user switched off.
//!
//! Deactivating is not hiding. A hidden tile's plugin keeps running and may
//! still push popups (that is `pluginsHidden`); a deactivated plugin has no
//! process at all, so it costs nothing, while its folder, its data directory
//! and its settings block stay untouched.
//!
//! The switch is one config list, `pluginsDeactivated`. Everything else in
//! this module only REACTS to that list, which is what keeps the MCP tool,
//! the Tauri command and a hand-edit of `config.json` on exactly one path:
//!
//! - [`set_plugin_active`] is the single writer.
//! - [`reconcile`] runs on every config change and stops or starts processes.
//! - [`gate`] is consulted by `start_plugin`, so the startup scan, the folder
//!   watcher and an explicit restart all refuse a deactivated plugin without
//!   having to remember to check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::{ConfigError, ConfigWatcher};

use super::manifest::PluginManifest;
use super::supervisor::{Inner, lifecycle_guard, start_plugin, stop_handle};
use crate::util::lock_unpoisoned;

use super::{PluginEvent, PluginStatus};

/// An installed plugin that must not run, keyed by manifest id.
#[derive(Debug, Clone)]
pub(super) struct DeactivatedPlugin {
    pub(super) dir: PathBuf,
    pub(super) manifest: PluginManifest,
}

/// Deactivated plugins, so they still show up in `plugin_infos` and can be
/// started again without re-reading the plugins directory.
pub(super) type DeactivatedMap = BTreeMap<String, DeactivatedPlugin>;

/// Adds or removes `id` in the config's `pluginsDeactivated` list and applies
/// the result. The supervisor's config subscription does the actual stopping
/// or starting, so this one function serves the MCP tool, the Tauri command
/// and any other caller alike.
///
/// Returns `true` when the list actually changed; `false` means the plugin
/// was already in the requested state and nothing was written.
pub fn set_plugin_active(
    config: &ConfigWatcher,
    plugin_id: &str,
    active: bool,
) -> Result<bool, ConfigError> {
    config.update(|current| {
        let listed = current.plugins_deactivated.iter().any(|id| id == plugin_id);
        if listed != active {
            return (current.clone(), false);
        }
        let mut updated = current.clone();
        if active {
            updated.plugins_deactivated.retain(|id| id != plugin_id);
        } else {
            updated.plugins_deactivated.push(plugin_id.to_string());
        }
        (updated, true)
    })
}

/// Whether `plugin_id` is switched off in the current config.
pub(super) fn is_deactivated(inner: &Inner, plugin_id: &str) -> bool {
    inner
        .config
        .current()
        .plugins_deactivated
        .iter()
        .any(|id| id == plugin_id)
}

/// The gate `start_plugin` calls once it has a valid manifest: records the
/// plugin as deactivated (instead of spawning it) and reports that it did.
///
/// Recording rather than ignoring is what keeps a deactivated plugin visible
/// in `plugin_infos` — the user has to be able to find it again to switch it
/// back on, and an agent has to be able to see why it is not running.
pub(super) fn gate(inner: &Arc<Inner>, dir: &Path, manifest: &PluginManifest) -> bool {
    if !is_deactivated(inner, &manifest.id) {
        return false;
    }
    tracing::debug!(
        plugin = %manifest.id,
        path = %dir.display(),
        "plugin is deactivated; not starting it"
    );
    record(inner, dir, manifest);
    true
}

/// Remembers a deactivated plugin and announces the state.
fn record(inner: &Arc<Inner>, dir: &Path, manifest: &PluginManifest) {
    lock_unpoisoned(&inner.deactivated).insert(
        manifest.id.clone(),
        DeactivatedPlugin {
            dir: dir.to_path_buf(),
            manifest: manifest.clone(),
        },
    );
    let events = inner.events.clone();
    let plugin_id = manifest.id.clone();
    // From a task, so a subscriber attaching right after `start()` (before
    // the first yield to the runtime) still sees this.
    tokio::spawn(async move {
        let _ = events.send(PluginEvent::Status {
            plugin_id,
            status: PluginStatus::Deactivated,
            error: None,
        });
    });
}

/// Forgets a plugin's deactivation record (it was deleted or its folder is
/// gone). Returns whether there was one.
pub(super) fn forget(inner: &Inner, plugin_id: &str) -> bool {
    lock_unpoisoned(&inner.deactivated)
        .remove(plugin_id)
        .is_some()
}

/// Whether the plugin behind `dir` is currently held as deactivated.
pub(super) fn deactivated_id_for_dir(inner: &Inner, dir: &Path) -> Option<String> {
    lock_unpoisoned(&inner.deactivated)
        .iter()
        .find_map(|(id, entry)| (entry.dir == dir).then(|| id.clone()))
}

/// Snapshot for [`super::status::merge_infos`].
pub(super) fn entries(inner: &Inner) -> Vec<(String, PathBuf, PluginManifest)> {
    lock_unpoisoned(&inner.deactivated)
        .iter()
        .map(|(id, entry)| (id.clone(), entry.dir.clone(), entry.manifest.clone()))
        .collect()
}

/// Applies a change of `pluginsDeactivated`: stops the processes of newly
/// deactivated plugins, starts the ones that were switched back on.
pub(super) async fn reconcile(inner: &Arc<Inner>, old: &[String], new: &[String]) {
    let previous: BTreeSet<&str> = old.iter().map(String::as_str).collect();
    let current: BTreeSet<&str> = new.iter().map(String::as_str).collect();
    for id in current.difference(&previous) {
        deactivate(inner, id).await;
    }
    for id in previous.difference(&current) {
        activate(inner, id).await;
    }
}

/// Stops a running plugin and files it as deactivated. An id that is not
/// running (never installed, or already off) is a no-op.
async fn deactivate(inner: &Arc<Inner>, plugin_id: &str) {
    let Some((_transition, _lifecycle)) = lifecycle_guard(inner, plugin_id).await else {
        return;
    };
    let handle = lock_unpoisoned(&inner.plugins).remove(plugin_id);
    let Some(handle) = handle else {
        tracing::debug!(
            plugin = %plugin_id,
            "deactivated an id that is not running; nothing to stop"
        );
        return;
    };
    let dir = handle.dir.clone();
    let manifest = handle.manifest.clone();
    tracing::info!(plugin = %plugin_id, "plugin deactivated; stopping its process");
    stop_handle(handle).await;
    // Removed BEFORE the status: it clears the cached renders and tells the
    // bar to drop the tiles, and the status event that follows is what
    // leaves `deactivated` (not `stopped`) as the last word.
    let _ = inner.events.send(PluginEvent::Removed {
        plugin_id: plugin_id.to_string(),
    });
    record(inner, &dir, &manifest);
}

/// Starts a plugin that was switched back on.
async fn activate(inner: &Arc<Inner>, plugin_id: &str) {
    let Some((_transition, _lifecycle)) = lifecycle_guard(inner, plugin_id).await else {
        return;
    };
    let entry = lock_unpoisoned(&inner.deactivated).remove(plugin_id);
    let Some(entry) = entry else {
        tracing::debug!(
            plugin = %plugin_id,
            "activated an id that was not deactivated; nothing to start"
        );
        return;
    };
    tracing::info!(plugin = %plugin_id, "plugin activated; starting its process");
    start_plugin(inner, &entry.dir);
}
