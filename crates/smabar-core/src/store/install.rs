//! The plugin install/update transaction: download, prove, swap, start —
//! and put everything back when the new version does not start.
//!
//! Nothing runs before the swap: download and extraction only read bytes.
//! The swap itself is [`crate::plugins::PluginSupervisor::replace_dir`]
//! under the plugin's lifecycle lock, journaled so a crash between its two
//! renames is repaired at the next start ([`super::journal::recover`]).

mod rollback;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use url::Url;

use crate::plugins::{PluginManifest, PluginRuntime, PluginStatus, is_valid_plugin_id};
use crate::util::{lock_unpoisoned, now_ms};

use super::archive::{ArchiveLimits, extract_plugin};
use super::catalog::{self, ItemKind, PluginEntry};
use super::compat;
use super::fetch::{MAX_ARCHIVE_BYTES, expected_archive_url, is_full_commit};
use super::journal::{self, Journal};
use super::receipts::{self, PluginReceipt};
use super::view::{CatalogState, InstallPhase, InstallProgress};
use super::{ChangeReason, Inner, StoreError, notify};

/// How long the new version may take to reach a terminal state. A python
/// plugin's FIRST run provisions its toolchain and can take longer; that
/// case is reported as `settled: false`, not as a failure.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Where the shell reports progress to.
pub type ProgressSink<'a> = &'a (dyn Fn(InstallProgress) + Send + Sync);

/// What an install or update produced.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub id: String,
    pub version: String,
    pub replaced_version: Option<String>,
    pub status: PluginStatus,
    pub error: Option<String>,
    /// A terminal status arrived within the wait; `false` means the plugin
    /// is still starting.
    pub settled: bool,
    pub backup: Option<PathBuf>,
}

/// Clears the "pending" marker however the transaction ends.
struct PendingGuard<'a> {
    inner: &'a Inner,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        lock_unpoisoned(&self.inner.state).pending = None;
    }
}

fn report(
    inner: &Inner,
    sink: ProgressSink<'_>,
    id: &str,
    phase: InstallPhase,
    received: u64,
    total: Option<u64>,
) {
    let progress = InstallProgress {
        kind: ItemKind::Plugin,
        id: id.to_string(),
        phase,
        received,
        total,
    };
    lock_unpoisoned(&inner.state).pending = Some(progress.clone());
    sink(progress);
}

/// Refreshes the catalog and insists on a fresh one: a download needs the
/// network anyway, and a cached listing must never authorize an install.
pub(super) async fn require_fresh(inner: &Inner) -> Result<(), StoreError> {
    super::refresh::refresh(inner).await;
    let state = lock_unpoisoned(&inner.state);
    if state.catalog_state == CatalogState::Fresh {
        return Ok(());
    }
    Err(StoreError::StoreUnreachable(
        state
            .last_error
            .clone()
            .unwrap_or_else(|| "no catalog".to_string()),
    ))
}

pub(super) async fn install_plugin(
    inner: &Inner,
    id: &str,
    expected_version: &str,
    confirm_modified: bool,
    sink: ProgressSink<'_>,
) -> Result<InstallOutcome, StoreError> {
    let _one_at_a_time = inner
        .install_lock
        .try_lock()
        .map_err(|_| StoreError::Busy)?;
    let _pending = PendingGuard { inner };
    if !is_valid_plugin_id(id) {
        return Err(StoreError::InvalidId { id: id.to_string() });
    }
    journal::ensure_idle(&inner.paths).map_err(|source| StoreError::Io {
        action: "install a plugin",
        path: inner.paths.store_journal_file(),
        source,
    })?;
    require_fresh(inner).await?;
    let (entry, blocked) = {
        let state = lock_unpoisoned(&inner.state);
        let listing = state.listing.as_ref().ok_or(StoreError::NoCatalog)?;
        let entry = listing
            .plugin(id)
            .cloned()
            .ok_or_else(|| StoreError::Unknown { id: id.to_string() })?;
        let blocked = listing
            .block_reason(ItemKind::Plugin, id, &entry.common.version)
            .map(str::to_string);
        (entry, blocked)
    };
    if entry.common.version != expected_version {
        return Err(StoreError::VersionChanged {
            id: id.to_string(),
            expected: expected_version.to_string(),
            actual: entry.common.version,
        });
    }
    let previous = guard(inner, &entry, blocked, confirm_modified)?;

    let staging = inner.paths.store_staging_dir();
    let staged_dir = staging.join(id);
    let zip_path = staging.join(format!("{id}.zip"));
    clean_staging(&staged_dir, &zip_path)?;
    let result = stage(inner, &entry, &staged_dir, &zip_path, sink).await;
    let _ = fs::remove_file(&zip_path);
    let installed_digest = match result {
        Ok(digest) => digest,
        Err(error) => {
            let _ = fs::remove_dir_all(&staged_dir);
            return Err(error);
        }
    };

    report(inner, sink, id, InstallPhase::Installing, 0, None);
    let dir = inner.paths.plugins_dir().join(id);
    let backup = inner.paths.store_backup_dir(id);
    let had_previous = dir.is_dir();
    let receipt = PluginReceipt {
        repo_id: entry.common.repo.id,
        name_with_owner: entry.common.repo.name_with_owner.clone(),
        path: entry.common.path.clone(),
        version: entry.common.version.clone(),
        commit: entry.source.commit.clone(),
        tree_oid: entry.source.tree_oid.clone(),
        installed_digest,
        installed_at: now_ms(),
        blocked: None,
        deactivated_by_store: previous
            .as_ref()
            .is_some_and(|previous| previous.deactivated_by_store),
    };
    let transaction = Journal {
        id: id.to_string(),
        had_previous,
        receipt: receipt.clone(),
        previous_receipt: previous.clone(),
    };
    journal::write(&inner.paths, &transaction)?;
    let watcher = inner.supervisor.watch_status(id);
    if let Err(error) = inner.supervisor.replace_dir(id, &staged_dir, &backup).await {
        return Err(error.into());
    }
    write_receipt(inner, id, Some(receipt)).await?;
    journal::clear(&inner.paths)?;

    report(inner, sink, id, InstallPhase::Starting, 0, None);
    if !inner.supervisor.hot_reload_enabled()
        && let Err(error) = inner.supervisor.restart(id).await
    {
        // Deactivated is the expected answer for a switched-off plugin.
        tracing::debug!(plugin = %id, %error, "explicit start after the swap was refused");
    }
    let fallback = inner
        .supervisor
        .plugin_infos()
        .into_iter()
        .find(|info| info.id == id)
        .map_or(PluginStatus::Starting, |info| info.status);
    let outcome = watcher.settle(SETTLE_TIMEOUT, fallback).await;
    if outcome.status == PluginStatus::Failed {
        let reason = outcome
            .error
            .unwrap_or_else(|| "no reason reported".to_string());
        let rolled_back = match rollback::run(inner, &transaction, &backup).await {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(plugin = %id, %error, "rollback could not finish; preserve the journal and backups, resolve the error and restart smabar");
                false
            }
        };
        notify(inner, ChangeReason::Install);
        return Err(StoreError::StartFailed {
            id: id.to_string(),
            reason,
            rolled_back,
        });
    }
    report(inner, sink, id, InstallPhase::Done, 0, None);
    tracing::info!(
        plugin = %id,
        version = %entry.common.version,
        replaced = ?previous.as_ref().map(|previous| previous.version.as_str()),
        "installed from the Community Store"
    );
    notify(inner, ChangeReason::Install);
    Ok(InstallOutcome {
        id: id.to_string(),
        version: entry.common.version,
        replaced_version: previous.map(|previous| previous.version),
        status: outcome.status,
        error: outcome.error,
        settled: outcome.settled,
        backup: had_previous.then_some(backup),
    })
}

/// Every reason not to touch the disk, checked before the download starts.
/// Returns the receipt of the installed copy that is about to be replaced.
fn guard(
    inner: &Inner,
    entry: &PluginEntry,
    blocked: Option<String>,
    confirm_modified: bool,
) -> Result<Option<PluginReceipt>, StoreError> {
    let id = &entry.common.id;
    if inner.reserved_ids.contains(id) {
        return Err(StoreError::BasePlugin { id: id.clone() });
    }
    if let Some(reason) = blocked {
        return Err(StoreError::Blocked {
            id: id.clone(),
            version: entry.common.version.clone(),
            reason,
        });
    }
    if !compat::os_allowed(&entry.common.requires.os, inner.host_os) {
        return Err(incompatible(
            inner,
            id,
            format!("one of: {}", entry.common.requires.os.join(", ")),
        ));
    }
    if !compat::smabar_satisfies(&inner.app_version, entry.common.requires.smabar.as_deref()) {
        return Err(incompatible(
            inner,
            id,
            format!(
                "smabar {} or newer",
                entry.common.requires.smabar.as_deref().unwrap_or("?")
            ),
        ));
    }
    let expected = expected_archive_url(&entry.common.repo.name_with_owner, &entry.source.commit);
    if entry.source.archive_url != expected || !is_full_commit(&entry.source.commit) {
        return Err(StoreError::Contract {
            id: id.clone(),
            detail: format!("archiveUrl {} is not {expected}", entry.source.archive_url),
        });
    }
    let dir = inner.paths.plugins_dir().join(id);
    let receipt = receipts::load(&inner.paths).plugins.get(id).cloned();
    if dir.exists() && receipt.is_none() {
        return Err(StoreError::UserPlugin {
            id: id.clone(),
            path: dir,
        });
    }
    if let Some(receipt) = &receipt
        && receipts::is_plugin_modified(&inner.paths, id, receipt)
        && !confirm_modified
    {
        return Err(StoreError::LocallyModified {
            id: id.clone(),
            backup: inner.paths.store_backup_dir(id),
        });
    }
    Ok(receipt)
}

fn incompatible(inner: &Inner, id: &str, requirement: String) -> StoreError {
    StoreError::Incompatible {
        id: id.to_string(),
        requirement,
        app_version: inner.app_version.to_string(),
        host_os: inner.host_os.unwrap_or("an unsupported system").to_string(),
    }
}

fn clean_staging(staged_dir: &Path, zip_path: &Path) -> Result<(), StoreError> {
    if staged_dir.exists() {
        fs::remove_dir_all(staged_dir).map_err(|source| StoreError::Io {
            action: "clear",
            path: staged_dir.to_path_buf(),
            source,
        })?;
    }
    if zip_path.exists() {
        fs::remove_file(zip_path).map_err(|source| StoreError::Io {
            action: "clear",
            path: zip_path.to_path_buf(),
            source,
        })?;
    }
    let staging = staged_dir.parent().unwrap_or(staged_dir);
    fs::create_dir_all(staging).map_err(|source| StoreError::Io {
        action: "create",
        path: staging.to_path_buf(),
        source,
    })
}

/// Downloads, extracts and proves the listed content; returns the digest
/// of the staged folder for the receipt.
async fn stage(
    inner: &Inner,
    entry: &PluginEntry,
    staged_dir: &Path,
    zip_path: &Path,
    sink: ProgressSink<'_>,
) -> Result<String, StoreError> {
    let id = entry.common.id.as_str();
    let url = Url::parse(&entry.source.archive_url).map_err(|error| StoreError::Contract {
        id: id.to_string(),
        detail: format!("archiveUrl is not a URL: {error}"),
    })?;
    report(inner, sink, id, InstallPhase::Downloading, 0, None);
    inner
        .fetcher
        .download(&url, zip_path, MAX_ARCHIVE_BYTES, &|received, total| {
            report(inner, sink, id, InstallPhase::Downloading, received, total);
        })
        .await?;

    report(inner, sink, id, InstallPhase::Verifying, 0, None);
    let repo_name = entry
        .common
        .repo
        .name_with_owner
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let extraction = {
        let archive_path = zip_path.to_path_buf();
        let commit = entry.source.commit.clone();
        let path = entry.common.path.clone();
        let staged_dir = staged_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            extract_plugin(
                &archive_path,
                &repo_name,
                &commit,
                &path,
                &staged_dir,
                &ArchiveLimits::DEFAULT,
            )
        })
        .await
        .map_err(|error| StoreError::Io {
            action: "extract",
            path: zip_path.to_path_buf(),
            source: std::io::Error::other(error),
        })??
    };
    if extraction.tree_oid != entry.source.tree_oid {
        return Err(StoreError::TreeMismatch {
            id: id.to_string(),
            expected: entry.source.tree_oid.clone(),
            actual: extraction.tree_oid,
        });
    }
    if catalog::sha256_hex(&extraction.manifest) != entry.source.manifest_sha256 {
        return Err(StoreError::DigestMismatch {
            what: "smabar.json",
            id: id.to_string(),
        });
    }
    let manifest_path = staged_dir.join(crate::plugins::MANIFEST_FILE);
    let manifest_text =
        String::from_utf8(extraction.manifest).map_err(|error| StoreError::Contract {
            id: id.to_string(),
            detail: format!("smabar.json is not UTF-8: {error}"),
        })?;
    let manifest = PluginManifest::parse(&manifest_text, &manifest_path)?;
    if manifest.id != id {
        return Err(StoreError::IdMismatch {
            expected: id.to_string(),
            actual: manifest.id,
        });
    }
    if manifest.version != entry.common.version {
        return Err(StoreError::VersionMismatch {
            id: id.to_string(),
            listed: entry.common.version.clone(),
            manifest: manifest.version,
        });
    }
    if manifest.runtime == PluginRuntime::Python {
        let entry_file = manifest.entry.clone().unwrap_or_default();
        let inside = !entry_file.is_empty()
            && !Path::new(&entry_file).is_absolute()
            && !entry_file.split(['/', '\\']).any(|part| part == "..")
            && staged_dir.join(&entry_file).is_file();
        if !inside {
            return Err(StoreError::EntryMissing {
                id: id.to_string(),
                entry: entry_file,
            });
        }
    }
    receipts::folder_digest(staged_dir).map_err(|source| StoreError::Io {
        action: "digest",
        path: staged_dir.to_path_buf(),
        source,
    })
}

/// Inserts (or, with `None`, removes) a plugin's receipt.
pub(super) async fn write_receipt(
    inner: &Inner,
    id: &str,
    receipt: Option<PluginReceipt>,
) -> Result<(), StoreError> {
    let _receipts = inner.receipts_lock.lock().await;
    let mut receipts = receipts::load_checked(&inner.paths).map_err(|source| StoreError::Io {
        action: "read receipts before saving the store transaction",
        path: inner.paths.store_receipts_file(),
        source,
    })?;
    match receipt {
        Some(receipt) => {
            receipts.plugins.insert(id.to_string(), receipt);
        }
        None => {
            receipts.plugins.remove(id);
        }
    }
    receipts::save(&inner.paths, &receipts).map_err(|source| StoreError::Io {
        action: "save the store receipt; resolve the write error and restart smabar",
        path: inner.paths.store_receipts_file(),
        source,
    })
}
