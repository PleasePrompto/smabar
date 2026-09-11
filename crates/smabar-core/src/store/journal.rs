//! The install journal: what an interrupted swap has to be brought back to.
//!
//! Written before the first rename of an install and removed after the
//! receipt is saved. At startup — before the orphan sweep, which would eat
//! the data of a plugin whose folder is mid-move — [`recover`] replays it:
//! the folder in place is either the new version (its digest matches the new
//! receipt), the old one, or missing with the backup waiting.

use std::fs;
use std::io;

use serde::{Deserialize, Serialize};

use crate::config::SmabarPaths;
use crate::util::write_atomically;

use super::StoreError;
use super::receipts::{self, PluginReceipt};

/// `store/transaction.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Journal {
    pub id: String,
    pub had_previous: bool,
    pub receipt: PluginReceipt,
    #[serde(default)]
    pub previous_receipt: Option<PluginReceipt>,
}

pub(super) fn write(paths: &SmabarPaths, journal: &Journal) -> Result<(), StoreError> {
    ensure_idle(paths).map_err(|source| StoreError::Io {
        action: "begin a store transaction",
        path: paths.store_journal_file(),
        source,
    })?;
    let json = serde_json::to_vec_pretty(journal).map_err(|source| StoreError::Io {
        action: "encode",
        path: paths.store_journal_file(),
        source: std::io::Error::other(source),
    })?;
    write_atomically(&paths.store_journal_file(), &json).map_err(|source| StoreError::Io {
        action: "write",
        path: paths.store_journal_file(),
        source,
    })
}

pub(super) fn clear(paths: &SmabarPaths) -> Result<(), StoreError> {
    let file = paths.store_journal_file();
    fs::remove_file(&file).map_err(|source| StoreError::Io {
        action: "clear the completed install journal",
        path: file,
        source,
    })
}

/// Unreadable paths and dangling symlinks also protect a pending transaction.
pub(crate) fn pending(paths: &SmabarPaths) -> bool {
    !matches!(fs::symlink_metadata(paths.store_journal_file()), Err(error) if error.kind() == io::ErrorKind::NotFound)
}

pub(crate) fn ensure_idle(paths: &SmabarPaths) -> io::Result<()> {
    if pending(paths) {
        return Err(io::Error::other(
            "store recovery is pending; preserve store/transaction.json and backups, resolve the recovery error in the log and restart smabar before installing or removing plugins",
        ));
    }
    Ok(())
}

fn read(paths: &SmabarPaths) -> Result<Option<Journal>, StoreError> {
    if !pending(paths) {
        return Ok(None);
    }
    let file = paths.store_journal_file();
    let raw = fs::read(&file).map_err(|source| StoreError::Io {
        action: "read the pending install journal",
        path: file.clone(),
        source,
    })?;
    let journal: Journal = serde_json::from_slice(&raw).map_err(|source| StoreError::Io {
        action: "decode the pending install journal; preserve it and its backup for recovery",
        path: file,
        source: io::Error::other(source),
    })?;
    if !crate::plugins::is_valid_plugin_id(&journal.id) {
        return Err(StoreError::InvalidId { id: journal.id });
    }
    Ok(Some(journal))
}

/// Completes or undoes an install that was interrupted by a crash, and
/// clears the staging area. Call it before the plugin supervisor starts and
/// before the orphan sweep. Returns the plugin id it acted on.
pub fn recover(paths: &SmabarPaths) -> Result<Option<String>, StoreError> {
    let staging = paths.store_staging_dir();
    let acted = if let Some(journal) = read(paths)? {
        replay(paths, &journal)?;
        clear(paths)?;
        Some(journal.id)
    } else {
        None
    };
    if staging.exists()
        && let Err(error) = fs::remove_dir_all(&staging)
    {
        tracing::warn!(path = %staging.display(), %error, "could not clear the store staging area");
    }
    Ok(acted)
}

fn replay(paths: &SmabarPaths, journal: &Journal) -> Result<(), StoreError> {
    let dir = paths.plugins_dir().join(&journal.id);
    let backup = paths.store_backup_dir(&journal.id);
    if !dir.is_dir()
        && !matches!(fs::symlink_metadata(&dir), Err(error) if error.kind() == io::ErrorKind::NotFound)
    {
        return Err(StoreError::Io {
            action: "recover the plugin; preserve the journal, resolve the non-directory obstruction and restart smabar",
            path: dir,
            source: io::Error::other("the plugin path is not a readable directory"),
        });
    }
    let in_place_is_new = dir.is_dir()
        && receipts::folder_digest(&dir).map_err(|source| StoreError::Io {
            action: "read the plugin during store recovery",
            path: dir.clone(),
            source,
        })? == journal.receipt.installed_digest;
    let mut receipts = receipts::load_checked(paths).map_err(|source| StoreError::Io {
        action: "read receipts during store recovery",
        path: paths.store_receipts_file(),
        source,
    })?;
    if in_place_is_new {
        tracing::info!(plugin = %journal.id, "completing an interrupted store install");
        receipts
            .plugins
            .insert(journal.id.clone(), journal.receipt.clone());
    } else {
        if !dir.is_dir() && journal.had_previous {
            if !backup.is_dir() {
                return Err(StoreError::Io {
                    action: "restore the previous plugin; preserve the journal and recover its backup directory before restarting smabar",
                    path: backup,
                    source: io::Error::other("the backup directory is missing or unreadable"),
                });
            }
            fs::create_dir_all(paths.plugins_dir()).and_then(|()| fs::rename(&backup, &dir))
                .map_err(|source| StoreError::Io {
                    action: "restore the previous plugin from backup; resolve the filesystem error and restart smabar",
                    path: backup,
                    source,
                })?;
            tracing::warn!(plugin = %journal.id, "restored the previous version after an interrupted store install");
        }
        match &journal.previous_receipt {
            Some(previous) if dir.is_dir() => {
                receipts
                    .plugins
                    .insert(journal.id.clone(), previous.clone());
            }
            _ => {
                receipts.plugins.remove(&journal.id);
            }
        }
    }
    receipts::save(paths, &receipts).map_err(|source| StoreError::Io {
        action: "save receipts during store recovery; resolve the write error and restart smabar",
        path: paths.store_receipts_file(),
        source,
    })
}
