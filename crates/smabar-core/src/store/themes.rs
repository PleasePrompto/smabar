//! Installing a Community Theme: one raw file from GitHub, verified by the
//! SHA-256 the catalog lists, written through the same path as a manual
//! theme import.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use url::Url;

use crate::themes::{is_bundled, is_valid_theme_name};
use crate::util::{lock_unpoisoned, now_ms};

use super::catalog::{self, ItemKind, ThemeEntry};
use super::fetch::{FetchOutcome, MAX_THEME_BYTES, expected_file_url, is_full_commit};
use super::receipts::{self, ThemeReceipt};
use super::refresh::theme_file;
use super::{ChangeReason, Inner, StoreError, notify};

/// What a theme install produced.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInstallOutcome {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    /// The installed theme is the active one.
    pub active: bool,
}

pub(super) async fn install_theme(
    inner: &Inner,
    name: &str,
    expected_version: &str,
) -> Result<ThemeInstallOutcome, StoreError> {
    let _one_at_a_time = inner
        .install_lock
        .try_lock()
        .map_err(|_| StoreError::Busy)?;
    if !is_valid_theme_name(name) {
        return Err(StoreError::InvalidId {
            id: name.to_string(),
        });
    }
    super::install::require_fresh(inner).await?;
    let (entry, blocked) = {
        let state = lock_unpoisoned(&inner.state);
        let listing = state.listing.as_ref().ok_or(StoreError::NoCatalog)?;
        let entry = listing
            .theme(name)
            .cloned()
            .ok_or_else(|| StoreError::Unknown {
                id: name.to_string(),
            })?;
        let blocked = listing
            .block_reason(ItemKind::Theme, name, &entry.common.version)
            .map(str::to_string);
        (entry, blocked)
    };
    if entry.common.version != expected_version {
        return Err(StoreError::VersionChanged {
            id: name.to_string(),
            expected: expected_version.to_string(),
            actual: entry.common.version,
        });
    }
    guard(inner, &entry, blocked)?;
    let url = Url::parse(&entry.source.file_url).map_err(|error| StoreError::Contract {
        id: name.to_string(),
        detail: format!("fileUrl is not a URL: {error}"),
    })?;
    let bytes = match inner.fetcher.get(&url, None, MAX_THEME_BYTES).await? {
        FetchOutcome::Body(body) => body.bytes,
        FetchOutcome::NotModified => Vec::new(),
    };
    if catalog::sha256_hex(&bytes) != entry.source.sha256 {
        return Err(StoreError::DigestMismatch {
            what: "the theme file",
            id: name.to_string(),
        });
    }
    let _receipts = inner.receipts_lock.lock().await;
    let mut receipts = receipts::load_checked(&inner.paths).map_err(|source| StoreError::Io {
        action: "read receipts before importing the theme; repair installed.json before retrying",
        path: inner.paths.store_receipts_file(),
        source,
    })?;
    let staging = inner.paths.store_staging_dir();
    fs::create_dir_all(&staging).map_err(|source| StoreError::Io {
        action: "create",
        path: staging.clone(),
        source,
    })?;
    let staged = staging.join(format!("{name}.json"));
    fs::write(&staged, &bytes).map_err(|source| StoreError::Io {
        action: "write",
        path: staged.clone(),
        source,
    })?;
    let written = crate::themes::io::import_theme_file(&inner.paths, &staged, true);
    let _ = fs::remove_file(&staged);
    let imported = written?;
    if imported != name {
        return Err(StoreError::Contract {
            id: name.to_string(),
            detail: format!("the theme file imported under the name \"{imported}\""),
        });
    }
    let path = theme_file(&inner.paths, name);
    let written_sha256 = fs::read(&path)
        .map(|bytes| catalog::sha256_hex(&bytes))
        .map_err(|source| StoreError::Io {
            action: "read back",
            path: path.clone(),
            source,
        })?;
    receipts.themes.insert(
        name.to_string(),
        ThemeReceipt {
            repo_id: entry.common.repo.id,
            name_with_owner: entry.common.repo.name_with_owner.clone(),
            path: entry.common.path.clone(),
            version: entry.common.version.clone(),
            commit: entry.source.commit.clone(),
            source_sha256: entry.source.sha256.clone(),
            written_sha256,
            installed_at: now_ms(),
            blocked: None,
        },
    );
    receipts::save(&inner.paths, &receipts).map_err(|source| StoreError::Io {
        action: "write",
        path: inner.paths.store_receipts_file(),
        source,
    })?;
    tracing::info!(theme = %name, version = %entry.common.version, "installed a theme from the Community Store");
    notify(inner, ChangeReason::Install);
    Ok(ThemeInstallOutcome {
        name: name.to_string(),
        version: entry.common.version,
        path,
        active: inner.config.current().theme == name,
    })
}

fn guard(inner: &Inner, entry: &ThemeEntry, blocked: Option<String>) -> Result<(), StoreError> {
    let name = &entry.common.id;
    if is_bundled(name) {
        return Err(StoreError::BundledTheme { name: name.clone() });
    }
    if let Some(reason) = blocked {
        return Err(StoreError::Blocked {
            id: name.clone(),
            version: entry.common.version.clone(),
            reason,
        });
    }
    let expected = expected_file_url(
        &entry.common.repo.name_with_owner,
        &entry.source.commit,
        &entry.common.path,
    );
    if entry.source.file_url != expected || !is_full_commit(&entry.source.commit) {
        return Err(StoreError::Contract {
            id: name.clone(),
            detail: format!("fileUrl {} is not {expected}", entry.source.file_url),
        });
    }
    let file = theme_file(&inner.paths, name);
    if file.exists() {
        let receipts = receipts::load(&inner.paths);
        let receipt = receipts
            .themes
            .get(name)
            .ok_or_else(|| StoreError::LocalTheme {
                name: name.clone(),
                path: file,
            })?;
        if super::view::update_view(
            &receipt.version,
            &entry.common.version,
            &receipt.source_sha256,
            &entry.source.sha256,
        )
        .is_none()
        {
            return Err(StoreError::NoUpdate { id: name.clone() });
        }
    }
    Ok(())
}
