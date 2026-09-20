//! Local ZIP installation shares the store's locks, safe extraction and recovery.
use super::{
    ChangeReason, InstallOutcome, InstallPhase, InstallProgress, ItemKind, ProgressSink,
    StoreError, StoreService, archive, catalog, compat, install, journal, notify, receipts,
};
use crate::config::SmabarPaths;
use crate::plugins::{PluginManifest, PluginRuntime, PluginStatus};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPluginPreview {
    pub id: String,
    pub name: String,
    pub version: String,
    pub archive_sha256: String,
    pub previous_digest: Option<String>,
    pub previous_version: Option<String>,
    pub community: bool,
}

struct Staged {
    dir: PathBuf,
    manifest: PluginManifest,
    sha256: String,
}
impl Drop for Staged {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.dir)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "could not clean local plugin staging; retry after resolving the filesystem error");
        }
    }
}
fn io(action: &'static str, path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}
fn prepare(paths: &SmabarPaths, path: &Path) -> Result<Staged, StoreError> {
    let file = fs::File::open(path).map_err(|error| io("open plugin ZIP", path, error))?;
    if !file
        .metadata()
        .map_err(|error| io("inspect plugin ZIP", path, error))?
        .is_file()
    {
        return Err(StoreError::LocalArchive("Choose a regular ZIP file".into()));
    }
    let mut bytes = Vec::new();
    file.take(super::fetch::MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io("read plugin ZIP", path, error))?;
    if bytes.len() as u64 > super::fetch::MAX_ARCHIVE_BYTES {
        return Err(StoreError::LocalArchive(
            "Plugin ZIP exceeds 50 MiB; select a smaller archive".into(),
        ));
    }
    let dir = paths.store_staging_dir().join(".local-import");
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| io("clear plugin staging", &dir, error))?;
    }
    let result = (|| {
        let extracted =
            archive::extract_local_plugin(&bytes, &dir, &archive::ArchiveLimits::DEFAULT)?;
        let raw = String::from_utf8(extracted.manifest).map_err(|error| {
            StoreError::LocalArchive(format!("smabar.json must be UTF-8: {error}"))
        })?;
        let manifest = PluginManifest::parse(&raw, &dir.join("smabar.json"))?;
        if manifest.runtime == PluginRuntime::Python {
            let entry = manifest.entry.as_deref().unwrap_or_default();
            if entry.is_empty()
                || Path::new(entry).is_absolute()
                || entry.split(['/', '\\']).any(|part| part == "..")
                || !dir.join(entry).is_file()
            {
                return Err(StoreError::EntryMissing {
                    id: manifest.id.clone(),
                    entry: entry.into(),
                });
            }
        }
        Ok(Staged {
            dir: dir.clone(),
            manifest,
            sha256: catalog::sha256_hex(&bytes),
        })
    })();
    if result.is_err()
        && let Err(error) = fs::remove_dir_all(&dir)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed ZIP inspection left staging files; retry after resolving the filesystem error");
    }
    result
}

impl StoreService {
    async fn prepare_local(&self, path: PathBuf) -> Result<Staged, StoreError> {
        journal::ensure_idle(&self.inner.paths).map_err(|error| {
            io(
                "prepare local install",
                &self.inner.paths.store_journal_file(),
                error,
            )
        })?;
        let paths = self.inner.paths.clone();
        tokio::task::spawn_blocking(move || prepare(&paths, &path))
            .await
            .map_err(|error| StoreError::LocalArchive(format!("ZIP inspection failed: {error}")))?
    }
    fn local_preview(&self, staged: &Staged) -> Result<LocalPluginPreview, StoreError> {
        let manifest = &staged.manifest;
        if self.inner.reserved_ids.contains(&manifest.id) {
            return Err(StoreError::BasePlugin {
                id: manifest.id.clone(),
            });
        }
        let raw = fs::read(staged.dir.join("smabar.json"))
            .map_err(|error| io("read staged manifest", &staged.dir, error))?;
        let value: serde_json::Value = serde_json::from_slice(&raw)
            .map_err(|error| StoreError::LocalArchive(error.to_string()))?;
        if let Some(store) = value.get("store") {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Requirements {
                os: Option<Vec<String>>,
                min_smabar: Option<String>,
            }
            let requirements: Requirements =
                serde_json::from_value(store.clone()).map_err(|error| {
                    StoreError::LocalArchive(format!(
                        "Invalid store compatibility metadata: {error}"
                    ))
                })?;
            if requirements
                .os
                .as_ref()
                .is_some_and(|os| !compat::os_allowed(os, self.inner.host_os))
                || !compat::smabar_satisfies(
                    &self.inner.app_version,
                    requirements.min_smabar.as_deref(),
                )
            {
                return Err(StoreError::LocalArchive(
                    "This plugin does not support this smabar version or operating system".into(),
                ));
            }
        }
        let state = crate::util::lock_unpoisoned(&self.inner.state);
        if let Some(reason) = state.listing.as_ref().and_then(|listing| {
            listing.block_reason(ItemKind::Plugin, &manifest.id, &manifest.version)
        }) {
            return Err(StoreError::Blocked {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
                reason: reason.into(),
            });
        }
        drop(state);
        let dir = self.inner.paths.plugins_dir().join(&manifest.id);
        let previous_digest = match fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() && !meta.is_symlink() => Some(
                receipts::folder_digest(&dir)
                    .map_err(|error| io("inspect installed plugin", &dir, error))?,
            ),
            Ok(_) => return Err(StoreError::LocalArchive(
                "The plugin destination is not a regular directory; resolve it before importing"
                    .into(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io("inspect plugin destination", &dir, error)),
        };
        let installed = receipts::load_checked(&self.inner.paths).map_err(|error| {
            io(
                "read plugin origin",
                &self.inner.paths.store_receipts_file(),
                error,
            )
        })?;
        Ok(LocalPluginPreview {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            archive_sha256: staged.sha256.clone(),
            previous_digest,
            previous_version: PluginManifest::load(&dir)
                .ok()
                .map(|manifest| manifest.version),
            community: installed.plugins.contains_key(&manifest.id),
        })
    }
    pub async fn inspect_local_plugin(
        &self,
        path: PathBuf,
    ) -> Result<LocalPluginPreview, StoreError> {
        let _lock = self
            .inner
            .install_lock
            .try_lock()
            .map_err(|_| StoreError::Busy)?;
        let staged = self.prepare_local(path).await?;
        self.local_preview(&staged)
    }
    pub async fn install_local_plugin(
        &self,
        path: PathBuf,
        archive_sha256: &str,
        previous_digest: Option<&str>,
        progress: ProgressSink<'_>,
    ) -> Result<InstallOutcome, StoreError> {
        let _lock = self
            .inner
            .install_lock
            .try_lock()
            .map_err(|_| StoreError::Busy)?;
        let staged = self.prepare_local(path).await?;
        let preview = self.local_preview(&staged)?;
        if staged.sha256 != archive_sha256 || preview.previous_digest.as_deref() != previous_digest
        {
            return Err(StoreError::LocalArchive(
                "The ZIP or installed plugin changed since confirmation; select the ZIP again"
                    .into(),
            ));
        }
        let id = &staged.manifest.id;
        let backup = self.inner.paths.store_backup_dir(id);
        let previous = receipts::load_checked(&self.inner.paths)
            .map_err(|error| {
                io(
                    "read plugin origin",
                    &self.inner.paths.store_receipts_file(),
                    error,
                )
            })?
            .plugins
            .get(id)
            .cloned();
        let transaction = journal::Journal {
            id: id.clone(),
            had_previous: previous_digest.is_some(),
            receipt: None,
            installed_digest: Some(
                receipts::folder_digest(&staged.dir)
                    .map_err(|error| io("verify staged plugin", &staged.dir, error))?,
            ),
            previous_receipt: previous,
        };
        let report = |phase| {
            progress(InstallProgress {
                kind: ItemKind::Plugin,
                id: id.clone(),
                phase,
                received: 0,
                total: None,
            })
        };
        report(InstallPhase::Installing);
        journal::write(&self.inner.paths, &transaction)?;
        let watcher = self.inner.supervisor.watch_status(id);
        self.inner
            .supervisor
            .replace_dir(id, &staged.dir, &backup)
            .await?;
        install::write_receipt(&self.inner, id, None).await?;
        journal::clear(&self.inner.paths)?;
        report(InstallPhase::Starting);
        if !self.inner.supervisor.hot_reload_enabled()
            && let Err(error) = self.inner.supervisor.restart(id).await
        {
            tracing::debug!(plugin = %id, %error, "local install explicit restart refused");
        }
        let fallback = self
            .inner
            .supervisor
            .plugin_infos()
            .into_iter()
            .find(|info| info.id == *id)
            .map_or(PluginStatus::Starting, |info| info.status);
        let outcome = watcher.settle(Duration::from_secs(15), fallback).await;
        if outcome.status == PluginStatus::Failed {
            let reason = outcome
                .error
                .unwrap_or_else(|| "No start reason reported".into());
            let rolled_back = match install::rollback::run(&self.inner, &transaction, &backup).await
            {
                Ok(()) => true,
                Err(error) => {
                    tracing::error!(plugin = %id, %error, "local update recovery failed; preserve the backup and journal");
                    false
                }
            };
            notify(&self.inner, ChangeReason::Install);
            return Err(StoreError::StartFailed {
                id: id.clone(),
                reason,
                rolled_back,
            });
        }
        report(InstallPhase::Done);
        notify(&self.inner, ChangeReason::Install);
        tracing::info!(plugin = %id, version = %staged.manifest.version, "installed local plugin ZIP");
        Ok(InstallOutcome {
            id: id.clone(),
            version: staged.manifest.version.clone(),
            replaced_version: preview.previous_version,
            status: outcome.status,
            error: outcome.error,
            settled: outcome.settled,
            backup: transaction.had_previous.then_some(backup),
        })
    }
}
