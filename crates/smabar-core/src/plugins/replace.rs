//! Swapping a plugin's code folder for a staged one — the Community Store's
//! install and update step, and its rollback.
//!
//! The swap is two renames under the plugin's lifecycle lock: the current
//! folder out to a backup, the staged folder in. Both land inside the same
//! debounce window of the folder watcher, which then applies ONE reload and
//! starts whatever is in place now — so nothing here starts the plugin while
//! hot reload works, or the new version would run twice.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use crate::util::lock_unpoisoned;

use super::supervisor::{PluginSupervisor, lifecycle_guard, start_plugin, stop_handle};

/// A just-stopped process can hold its folder open for a moment (Windows
/// refuses the rename until every handle is closed).
const RENAME_ATTEMPTS: u32 = 10;
const RENAME_RETRY: Duration = Duration::from_millis(200);

/// Errors from [`PluginSupervisor::replace_dir`].
#[derive(Debug, Error)]
pub enum ReplaceError {
    #[error("plugin id \"{id}\" must be non-empty and contain only [a-z0-9-]")]
    InvalidId { id: String },
    #[error("cannot replace plugin \"{id}\" while the plugin supervisor is shutting down")]
    ShuttingDown { id: String },
    #[error("the staged plugin folder {path} does not exist")]
    StagedMissing { path: PathBuf },
    /// Naming the folder in use is the actionable part: on Windows the user
    /// has to close whatever holds it.
    #[error(
        "cannot {action} {path}: {source}; close programs using ~/.smabar/plugins and keep \
         ~/.smabar/plugins and ~/.smabar/store on the same filesystem"
    )]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// What [`PluginSupervisor::replace_dir`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceOutcome {
    /// The plugin folder now holding the staged content.
    pub dir: PathBuf,
    /// A previous folder existed and was moved to the backup path.
    pub backed_up: bool,
    /// A process was running and has been stopped.
    pub was_running: bool,
}

impl PluginSupervisor {
    /// Stops the plugin, moves its current folder to `backup` (replacing an
    /// older backup there) and moves `staged` into `plugins/<id>/`.
    ///
    /// A deactivated plugin stays deactivated. When the second rename fails
    /// the backup is moved back and the previous version keeps running.
    pub async fn replace_dir(
        &self,
        plugin_id: &str,
        staged: &Path,
        backup: &Path,
    ) -> Result<ReplaceOutcome, ReplaceError> {
        if !super::is_valid_plugin_id(plugin_id) {
            return Err(ReplaceError::InvalidId {
                id: plugin_id.to_string(),
            });
        }
        if !staged.is_dir() {
            return Err(ReplaceError::StagedMissing {
                path: staged.to_path_buf(),
            });
        }
        let inner = Arc::clone(&self.inner);
        let Some((_transition, _lifecycle)) = lifecycle_guard(&inner, plugin_id).await else {
            return Err(ReplaceError::ShuttingDown {
                id: plugin_id.to_string(),
            });
        };
        let dir = inner.paths.plugins_dir().join(plugin_id);
        let handle = lock_unpoisoned(&inner.plugins).remove(plugin_id);
        let was_running = handle.is_some();
        if let Some(handle) = handle {
            tracing::info!(plugin = %plugin_id, "replacing the plugin folder; stopping the current instance");
            stop_handle(handle).await;
        }
        let restore_supervision = |inner: &Arc<super::supervisor::Inner>| {
            if was_running && dir.is_dir() {
                start_plugin(inner, &dir);
            }
        };

        let had_previous = dir.is_dir();
        if had_previous {
            if backup.exists()
                && let Err(source) = fs::remove_dir_all(backup)
            {
                restore_supervision(&inner);
                return Err(ReplaceError::Io {
                    action: "clear the backup folder",
                    path: backup.to_path_buf(),
                    source,
                });
            }
            if let Some(parent) = backup.parent()
                && let Err(source) = fs::create_dir_all(parent)
            {
                restore_supervision(&inner);
                return Err(ReplaceError::Io {
                    action: "create",
                    path: parent.to_path_buf(),
                    source,
                });
            }
            if let Err(source) = rename_with_retry(&dir, backup).await {
                restore_supervision(&inner);
                return Err(ReplaceError::Io {
                    action: "move away",
                    path: dir.clone(),
                    source,
                });
            }
        }
        if let Err(source) = fs::rename(staged, &dir) {
            if had_previous && let Err(error) = fs::rename(backup, &dir) {
                tracing::error!(
                    plugin = %plugin_id,
                    %error,
                    backup = %backup.display(),
                    "could not move the previous version back; restore it by hand from the backup"
                );
            }
            restore_supervision(&inner);
            return Err(ReplaceError::Io {
                action: "move into place",
                path: staged.to_path_buf(),
                source,
            });
        }
        tracing::info!(
            plugin = %plugin_id,
            path = %dir.display(),
            backed_up = had_previous,
            "replaced the plugin folder"
        );
        // Without a folder watch nothing would start the new version.
        if lock_unpoisoned(&inner.watcher).is_none() {
            start_plugin(&inner, &dir);
        }
        Ok(ReplaceOutcome {
            dir,
            backed_up: had_previous,
            was_running,
        })
    }
}

async fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < RENAME_ATTEMPTS && error.kind() == io::ErrorKind::PermissionDenied =>
            {
                attempt += 1;
                tokio::time::sleep(RENAME_RETRY).await;
            }
            Err(error) => return Err(error),
        }
    }
}
