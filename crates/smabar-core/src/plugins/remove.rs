//! Permanent removal of a plugin: code, data, logs, and its deactivation flag.
//!
//! This is the irreversible one of the three lifecycle actions (hide,
//! deactivate, delete), so it lives in exactly one place and both frontends
//! call it. The data and log cleanup is deliberately NOT re-implemented here:
//! the same cleanup as the startup orphan sweep removes `data/<id>/` and
//! `logs/plugin-<id>.log`, restricted to this id under its lifecycle lock.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::util::lock_unpoisoned;

use super::supervisor::{PluginSupervisor, lifecycle_guard, start_plugin, stop_handle};
use super::{PluginEvent, activation};

/// Errors from [`remove_plugin`].
#[derive(Debug, Error)]
pub enum RemoveError {
    /// The id is not a usable path segment.
    #[error("plugin id \"{id}\" must be non-empty and contain only [a-z0-9-]")]
    InvalidId { id: String },
    /// Nothing is installed under that id.
    #[error("no plugin \"{id}\": no folder {path} exists")]
    NotInstalled { id: String, path: PathBuf },
    /// Shutdown won the terminal lifecycle barrier.
    #[error("cannot delete plugin \"{id}\" while the plugin supervisor is shutting down")]
    ShuttingDown { id: String },
    /// The code folder could not be fully deleted; data/config cleanup was not attempted.
    #[error("cannot delete {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// What [`remove_plugin`] destroyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRemoval {
    /// The code folder that was deleted.
    pub dir: PathBuf,
    /// Plugin ids whose data directory and/or log files the sweep removed.
    /// Contains only this plugin when it had either.
    pub swept: Vec<String>,
    /// The plugin was switched off and that entry was dropped from
    /// `pluginsDeactivated` — otherwise reinstalling it later would silently
    /// produce a plugin that refuses to start.
    pub deactivation_cleared: bool,
    /// Tile ids of this plugin that were dropped from `pluginOrder` and
    /// `pluginsHidden`.
    pub order_entries_cleared: Vec<String>,
    /// The plugin came from the Community Store; its install receipt and
    /// backup went with it.
    pub store_receipt_cleared: bool,
}

impl PluginSupervisor {
    /// Stops a plugin under its lifecycle lock, then permanently removes its
    /// code, data, logs and stale config ids. A failed filesystem deletion
    /// restores supervision from whatever remains on disk.
    pub async fn remove(&self, plugin_id: &str) -> Result<PluginRemoval, RemoveError> {
        remove_supervised(self, plugin_id).await
    }
}

async fn remove_supervised(
    supervisor: &PluginSupervisor,
    plugin_id: &str,
) -> Result<PluginRemoval, RemoveError> {
    if !super::is_valid_plugin_id(plugin_id) {
        return Err(RemoveError::InvalidId {
            id: plugin_id.to_string(),
        });
    }
    let inner = Arc::clone(&supervisor.inner);
    let Some((_transition, _lifecycle)) = lifecycle_guard(&inner, plugin_id).await else {
        return Err(RemoveError::ShuttingDown {
            id: plugin_id.to_string(),
        });
    };
    let Some(dir) = supervisor.plugin_dir(plugin_id) else {
        return Err(RemoveError::NotInstalled {
            id: plugin_id.to_string(),
            path: inner.paths.plugins_dir().join(plugin_id),
        });
    };

    let handle = lock_unpoisoned(&inner.plugins).remove(plugin_id);
    if let Some(handle) = handle {
        tracing::info!(plugin = %plugin_id, "plugin removal requested; stopping its process");
        stop_handle(handle).await;
    }
    activation::forget(&inner, plugin_id);

    match remove_plugin(&inner.paths, &inner.config, plugin_id, &dir) {
        Ok(removal) => {
            inner.diagnostics.forget(plugin_id);
            let _ = inner.events.send(PluginEvent::Removed {
                plugin_id: plugin_id.to_string(),
            });
            Ok(removal)
        }
        Err(error) => {
            if dir.is_dir() {
                start_plugin(&inner, &dir);
            }
            Err(error)
        }
    }
}

/// Deletes a plugin's code folder, data, logs and stale config ids.
///
/// The per-plugin settings block in `config.plugins` is deliberately KEPT —
/// it is small, it carries the user's own configuration, and reinstalling the
/// plugin restores it. Runtime callers must use [`super::PluginSupervisor::remove`]
/// so the process is stopped before this filesystem layer runs.
pub fn remove_plugin(
    paths: &SmabarPaths,
    config: &ConfigWatcher,
    plugin_id: &str,
    dir: &Path,
) -> Result<PluginRemoval, RemoveError> {
    crate::store::journal::ensure_idle(paths).map_err(|source| RemoveError::Io {
        path: paths.store_journal_file(),
        source,
    })?;
    if !super::is_valid_plugin_id(plugin_id) {
        return Err(RemoveError::InvalidId {
            id: plugin_id.to_string(),
        });
    }
    if !dir.is_dir() {
        return Err(RemoveError::NotInstalled {
            id: plugin_id.to_string(),
            path: dir.to_path_buf(),
        });
    }
    fs::remove_dir_all(dir).map_err(|source| RemoveError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    tracing::info!(
        plugin_id = %plugin_id,
        path = %dir.display(),
        "permanently deleted a plugin's code folder"
    );
    // Another id may be mid-install on a different lifecycle lock.
    let swept = super::seed::remove_plugin_data(paths, plugin_id);
    let deactivation_cleared = clear_deactivation(config, plugin_id);
    let order_entries_cleared = clear_tile_ids(config, plugin_id);
    // A Community Plugin's receipt and backup belong to the folder; without
    // the receipt the id is free for the user's own plugin again.
    let store_receipt_cleared = crate::store::receipts::forget_plugin(paths, plugin_id);
    Ok(PluginRemoval {
        dir: dir.to_path_buf(),
        swept,
        deactivation_cleared,
        order_entries_cleared,
        store_receipt_cleared,
    })
}

/// Drops a deleted plugin's tile ids from `pluginOrder` and
/// `pluginsHidden`.
///
/// They are inert once the plugin is gone — `sortTiles` never matches them
/// — but they stay in `config.json` forever and the lists grow without
/// bound. The one case where they are not merely untidy is reinstalling the
/// same id: a stale `pluginsHidden` entry would bring the tile back
/// already hidden, with nothing to explain why.
fn clear_tile_ids(config: &ConfigWatcher, plugin_id: &str) -> Vec<String> {
    let prefix = format!("plugin:{plugin_id}:");
    let updated = config.update(|current| {
        let mut cleared: Vec<String> = current
            .plugin_order
            .iter()
            .chain(current.plugins_hidden.iter())
            .filter(|id| id.starts_with(&prefix))
            .cloned()
            .collect();
        if cleared.is_empty() {
            return (current.clone(), cleared);
        }
        cleared.sort();
        cleared.dedup();
        let mut updated = current.clone();
        updated.plugin_order.retain(|id| !id.starts_with(&prefix));
        updated.plugins_hidden.retain(|id| !id.starts_with(&prefix));
        (updated, cleared)
    });
    match updated {
        Ok(cleared) => cleared,
        Err(error) => {
            // The files are already gone; failing the call now would be worse
            // than a stale list that only matters on a reinstall.
            tracing::warn!(
                plugin_id = %plugin_id,
                %error,
                "deleted the plugin but could not drop its tile ids from \"pluginOrder\"/\"pluginsHidden\""
            );
            Vec::new()
        }
    }
}

/// Drops a deleted plugin from `pluginsDeactivated`. A failure here is worth
/// a warning but not an error: the plugin is already gone, and the stale
/// entry only matters if the very same id is installed again.
fn clear_deactivation(config: &ConfigWatcher, plugin_id: &str) -> bool {
    match super::set_plugin_active(config, plugin_id, true) {
        Ok(changed) => changed,
        Err(error) => {
            tracing::warn!(
                plugin_id = %plugin_id,
                %error,
                "deleted the plugin but could not drop it from \"pluginsDeactivated\"; \
                 remove that entry by hand before installing this id again"
            );
            false
        }
    }
}
