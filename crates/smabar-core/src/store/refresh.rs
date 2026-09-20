//! Fetching the catalog: conditional GET, signature check, the on-disk cache,
//! and the two things a fresh catalog changes locally — the blocklist and
//! stale receipts.

use std::fs;

use minisign_verify::PublicKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::update::set_config_path_activating;
use crate::plugins::set_plugin_active;
use crate::util::{lock_unpoisoned, now_ms, write_atomically};

use super::catalog::{self, Detail, ItemKind, Listing, verify_and_parse};
use super::fetch::{FetchOutcome, MAX_CATALOG_BYTES, MAX_DETAIL_BYTES, MAX_SIGNATURE_BYTES};
use super::readme::{ReadmeBase, render_readme};
use super::receipts::{self, BlockedState};
use super::view::{CatalogState, StoreDetail};
use super::{CatalogError, ChangeReason, Inner, StoreError, notify};

const CATALOG_FILE: &str = "catalog-v1.json";
const SIGNATURE_FILE: &str = "catalog-v1.json.sig";

/// `store/catalog.meta.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogMeta {
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    fetched_at: Option<u64>,
}

/// A catalog read back from disk that still verifies.
pub(super) struct CachedCatalog {
    pub(super) listing: Listing,
    pub(super) body_sha256: String,
    pub(super) etag: Option<String>,
    pub(super) fetched_at: Option<u64>,
}

/// Loads the cached catalog if every byte of it still verifies against
/// `key`; anything else is deleted, because a cache nobody can vouch for is
/// worse than none.
pub(super) fn load_cached(
    paths: &crate::config::SmabarPaths,
    key: &PublicKey,
) -> Option<CachedCatalog> {
    let catalog_file = paths.store_catalog_file();
    let bytes = fs::read(&catalog_file).ok()?;
    let signature = fs::read_to_string(paths.store_catalog_signature_file()).ok()?;
    match verify_and_parse(&bytes, &signature, key) {
        Ok(listing) => {
            let meta: CatalogMeta = fs::read_to_string(paths.store_catalog_meta_file())
                .ok()
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_default();
            Some(CachedCatalog {
                listing,
                body_sha256: catalog::sha256_hex(&bytes),
                etag: meta.etag,
                fetched_at: meta.fetched_at,
            })
        }
        Err(error) => {
            tracing::warn!(
                path = %catalog_file.display(),
                %error,
                "discarding the cached catalog; it no longer verifies"
            );
            discard_cache(paths);
            None
        }
    }
}

fn discard_cache(paths: &crate::config::SmabarPaths) {
    for file in [
        paths.store_catalog_file(),
        paths.store_catalog_signature_file(),
        paths.store_catalog_meta_file(),
    ] {
        if file.exists()
            && let Err(error) = fs::remove_file(&file)
        {
            tracing::warn!(path = %file.display(), %error, "could not remove a stale catalog file");
        }
    }
}

fn persist_cache(
    paths: &crate::config::SmabarPaths,
    bytes: &[u8],
    signature: &str,
    meta: &CatalogMeta,
) {
    let written = write_atomically(&paths.store_catalog_file(), bytes)
        .and_then(|()| {
            write_atomically(&paths.store_catalog_signature_file(), signature.as_bytes())
        })
        .and_then(|()| {
            let json = serde_json::to_vec_pretty(meta).map_err(std::io::Error::other)?;
            write_atomically(&paths.store_catalog_meta_file(), &json)
        });
    if let Err(error) = written {
        // A torn set fails verification at the next start and is discarded;
        // the in-memory catalog is unaffected.
        tracing::warn!(%error, "could not cache the catalog; it will be fetched again at the next start");
    }
}

pub(super) async fn refresh(inner: &Inner) -> super::StoreOverview {
    let _serialized = inner.refresh_lock.lock().await;
    let etag = lock_unpoisoned(&inner.state).etag.clone();
    let result = fetch_catalog(inner, etag.as_deref()).await;
    match result {
        Ok(()) => {
            reconcile_blocklist(inner).await;
            sweep_orphan_receipts(inner).await;
        }
        Err(error) => {
            let mut state = lock_unpoisoned(&inner.state);
            state.catalog_state = if state.listing.is_some() {
                CatalogState::Stale
            } else {
                CatalogState::Unavailable
            };
            state.last_error = Some(error.to_string());
            tracing::warn!(
                endpoint = %inner.endpoint,
                %error,
                "could not refresh the Community Catalog; showing the last verified one"
            );
        }
    }
    notify(inner, ChangeReason::Refresh);
    super::overview(inner)
}

async fn fetch_catalog(inner: &Inner, etag: Option<&str>) -> Result<(), StoreError> {
    let catalog_url = join(inner, CATALOG_FILE)?;
    let body = match inner
        .fetcher
        .get(&catalog_url, etag, MAX_CATALOG_BYTES)
        .await?
    {
        FetchOutcome::NotModified => {
            mark_fresh(inner, None, None);
            return Ok(());
        }
        FetchOutcome::Body(body) => body,
    };
    let sha256 = catalog::sha256_hex(&body.bytes);
    let unchanged = {
        let state = lock_unpoisoned(&inner.state);
        state.listing.is_some() && state.body_sha256.as_deref() == Some(sha256.as_str())
    };
    if unchanged {
        // The CDN rewrote the ETag but the bytes are the ones already verified.
        mark_fresh(inner, body.etag, None);
        return Ok(());
    }
    let signature_url = join(inner, SIGNATURE_FILE)?;
    let signature = match inner
        .fetcher
        .get(&signature_url, None, MAX_SIGNATURE_BYTES)
        .await?
    {
        FetchOutcome::Body(signature) => String::from_utf8_lossy(&signature.bytes).into_owned(),
        FetchOutcome::NotModified => {
            return Err(StoreError::Fetch(super::FetchError::Other(
                "the signature endpoint answered 304 to an unconditional request".to_string(),
            )));
        }
    };
    let listing = verify_and_parse(&body.bytes, &signature, &inner.key)?;
    let fetched_at = now_ms();
    persist_cache(
        &inner.paths,
        &body.bytes,
        &signature,
        &CatalogMeta {
            etag: body.etag.clone(),
            fetched_at: Some(fetched_at),
        },
    );
    tracing::info!(
        items = listing.items.len(),
        blocked = listing.blocklist.len(),
        generated_at = %listing.generated_at,
        "fetched and verified the Community Catalog"
    );
    {
        let mut state = lock_unpoisoned(&inner.state);
        state.listing = Some(listing);
        state.body_sha256 = Some(sha256);
        state.details.clear();
        state.theme_files.clear();
    }
    mark_fresh(inner, body.etag, Some(fetched_at));
    Ok(())
}

fn mark_fresh(inner: &Inner, etag: Option<String>, fetched_at: Option<u64>) {
    let mut state = lock_unpoisoned(&inner.state);
    if etag.is_some() {
        state.etag = etag;
    }
    state.fetched_at = Some(fetched_at.unwrap_or_else(now_ms));
    state.catalog_state = CatalogState::Fresh;
    state.last_error = None;
}

fn join(inner: &Inner, file: &str) -> Result<url::Url, StoreError> {
    inner
        .endpoint
        .join(file)
        .map_err(|error| StoreError::Endpoint(error.to_string()))
}

/// Applies the fresh catalog's blocklist to what is installed: a blocked
/// version is switched off (never deleted) and remembered; a block that a
/// NEWER catalog no longer carries is lifted, and the plugin comes back only
/// if it was the store that switched it off.
pub(super) async fn reconcile_blocklist(inner: &Inner) {
    let _receipts = inner.receipts_lock.lock().await;
    let listing = match lock_unpoisoned(&inner.state).listing.clone() {
        Some(listing) => listing,
        None => return,
    };
    let mut receipts = receipts::load(&inner.paths);
    let mut changed = false;
    let deactivated = inner.config.current().plugins_deactivated;
    for (id, receipt) in &mut receipts.plugins {
        let block = listing.block_entry(ItemKind::Plugin, id, &receipt.version);
        match (block, receipt.blocked.as_ref()) {
            (Some(block), _) => {
                let state = BlockedState {
                    reason: block.reason.clone(),
                    version: block.version.clone(),
                    catalog_generated_at: listing.generated_at.clone(),
                };
                if receipt.blocked.as_ref() != Some(&state) {
                    receipt.blocked = Some(state);
                    changed = true;
                }
                if !deactivated.iter().any(|entry| entry == id) {
                    match set_plugin_active(&inner.config, id, false) {
                        Ok(_) => {
                            receipt.deactivated_by_store = true;
                            changed = true;
                            tracing::warn!(
                                plugin = %id,
                                version = %receipt.version,
                                reason = %block.reason,
                                "blocked by the Community Store; switched off, nothing deleted"
                            );
                        }
                        Err(error) => tracing::error!(
                            plugin = %id,
                            %error,
                            "the store blocked this plugin but it could not be switched off; switch it off in Settings › Plugins"
                        ),
                    }
                }
            }
            (None, Some(blocked)) if listing.generated_at > blocked.catalog_generated_at => {
                receipt.blocked = None;
                changed = true;
                if receipt.deactivated_by_store {
                    receipt.deactivated_by_store = false;
                    match set_plugin_active(&inner.config, id, true) {
                        Ok(_) => {
                            tracing::info!(plugin = %id, "the store lifted its block; switched back on")
                        }
                        Err(error) => {
                            tracing::warn!(plugin = %id, %error, "block lifted but the plugin could not be switched back on")
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for (name, receipt) in &mut receipts.themes {
        let block = listing.block_entry(ItemKind::Theme, name, &receipt.version);
        match (block, receipt.blocked.as_ref()) {
            (Some(block), _) => {
                let state = BlockedState {
                    reason: block.reason.clone(),
                    version: block.version.clone(),
                    catalog_generated_at: listing.generated_at.clone(),
                };
                if receipt.blocked.as_ref() != Some(&state) {
                    receipt.blocked = Some(state);
                    changed = true;
                    tracing::warn!(theme = %name, reason = %block.reason, "blocked by the Community Store");
                }
                if inner.config.current().theme == *name {
                    switch_to_default_theme(inner, name);
                }
            }
            (None, Some(blocked)) if listing.generated_at > blocked.catalog_generated_at => {
                receipt.blocked = None;
                changed = true;
            }
            _ => {}
        }
    }
    if changed && let Err(error) = receipts::save(&inner.paths, &receipts) {
        tracing::warn!(%error, "could not persist block state in store/installed.json");
    }
}

fn switch_to_default_theme(inner: &Inner, blocked: &str) {
    let paths = inner.paths.clone();
    let result = inner.config.update(|current| {
        match set_config_path_activating(
            &paths,
            current,
            "theme",
            Value::String("default".to_string()),
        ) {
            Ok((updated, _)) => (updated, true),
            Err(_) => (current.clone(), false),
        }
    });
    match result {
        Ok(true) => {
            tracing::warn!(theme = %blocked, "the active theme is blocked by the store; switched to default")
        }
        Ok(false) | Err(_) => {
            tracing::warn!(theme = %blocked, "the active theme is blocked by the store but could not be switched; pick another theme in Settings › Design")
        }
    }
}

/// Receipts whose folder or file is gone (deleted by hand) are dropped,
/// together with their backup.
pub(super) async fn sweep_orphan_receipts(inner: &Inner) {
    let _receipts = inner.receipts_lock.lock().await;
    let receipts = receipts::load(&inner.paths);
    // A running install can temporarily move a plugin folder out of place.
    if let Ok(_install) = inner.install_lock.try_lock() {
        let orphans: Vec<String> = receipts
            .plugins
            .keys()
            .filter(|id| !inner.paths.plugins_dir().join(id).is_dir())
            .cloned()
            .collect();
        for id in orphans {
            if receipts::forget_plugin(&inner.paths, &id) {
                tracing::info!(plugin = %id, "its folder is gone; dropped the store receipt");
            }
        }
    }
    let theme_orphans: Vec<String> = receipts
        .themes
        .keys()
        .filter(|name| !theme_file(&inner.paths, name).is_file())
        .cloned()
        .collect();
    for name in theme_orphans {
        tracing::info!(theme = %name, "its file is gone; dropping the store receipt");
        receipts::forget_theme(&inner.paths, &name);
    }
}

pub(super) fn theme_file(paths: &crate::config::SmabarPaths, name: &str) -> std::path::PathBuf {
    paths.themes_dir().join(format!("{name}.json"))
}

pub(super) async fn detail(
    inner: &Inner,
    kind: ItemKind,
    id: &str,
) -> Result<StoreDetail, StoreError> {
    let (sha256, cached) = {
        let state = lock_unpoisoned(&inner.state);
        let listing = state.listing.as_ref().ok_or(StoreError::NoCatalog)?;
        let sha256 = listing
            .items
            .iter()
            .find(|entry| entry.kind() == kind && entry.common().id == id)
            .map(|entry| entry.common().detail_sha256.clone())
            .ok_or_else(|| StoreError::Unknown { id: id.to_string() })?;
        let cached = state.details.get(&sha256).cloned();
        (sha256, cached)
    };
    let detail = match cached {
        Some(detail) => detail,
        None => {
            let kind_segment = match kind {
                ItemKind::Plugin => "plugin",
                ItemKind::Theme => "theme",
            };
            let url = join(inner, &format!("items/{kind_segment}/{id}.json"))?;
            let bytes = match inner.fetcher.get(&url, None, MAX_DETAIL_BYTES).await? {
                FetchOutcome::Body(body) => body.bytes,
                FetchOutcome::NotModified => Vec::new(),
            };
            if catalog::sha256_hex(&bytes) != sha256 {
                return Err(StoreError::DigestMismatch {
                    what: "the detail file",
                    id: id.to_string(),
                });
            }
            let detail: Detail = serde_json::from_slice(&bytes).map_err(CatalogError::Parse)?;
            lock_unpoisoned(&inner.state)
                .details
                .insert(sha256, detail.clone());
            detail
        }
    };
    let entry = super::overview(inner)
        .entries
        .into_iter()
        .find(|entry| entry.kind == kind && entry.id == id)
        .ok_or_else(|| StoreError::Unknown { id: id.to_string() })?;
    let readme_html = detail
        .readme
        .as_deref()
        .map(|markdown| render_readme(markdown, &ReadmeBase::for_entry(&entry)));
    Ok(StoreDetail {
        entry,
        readme: detail.readme,
        readme_html,
        releases: detail.releases,
    })
}

#[cfg(test)]
pub(super) fn cache_files_exist(paths: &crate::config::SmabarPaths) -> bool {
    paths.store_catalog_file().is_file()
        && paths.store_catalog_signature_file().is_file()
        && paths.store_catalog_meta_file().is_file()
}
