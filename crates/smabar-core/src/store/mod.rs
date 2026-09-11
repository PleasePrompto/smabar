//! The Community Store client: the signed catalog from `store.smabar.com`,
//! what is installed from it, and the install/update transaction.
//!
//! One logic layer for every frontend: the Tauri commands and the MCP tools
//! call [`StoreService`]. The network is behind [`fetch::StoreFetcher`],
//! implemented at the app edge, so the module is testable with an in-memory
//! fake. Store state lives in `~/.smabar/store/`, never in `config.json`.

pub mod archive;
pub mod catalog;
pub mod compat;
pub mod fetch;
mod install;
pub(crate) mod journal;
mod readme;
pub mod receipts;
mod refresh;
mod themes;
pub mod treeoid;
pub mod view;

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod compat_tests;
#[cfg(test)]
mod install_guard_tests;
#[cfg(test)]
mod install_tests;
#[cfg(test)]
mod journal_tests;
#[cfg(test)]
mod readme_tests;
#[cfg(test)]
mod receipts_tests;
#[cfg(test)]
mod refresh_tests;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod themes_tests;
#[cfg(test)]
mod treeoid_tests;
#[cfg(test)]
mod view_tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use minisign_verify::PublicKey;
use semver::Version;
use serde::Serialize;
use tokio::sync::broadcast;
use url::Url;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::plugins::PluginSupervisor;
use crate::util::lock_unpoisoned;

pub use catalog::{CatalogError, ItemKind};
pub use fetch::{FetchError, StoreFetcher};
pub use install::{InstallOutcome, ProgressSink};
pub use journal::recover;
pub use themes::ThemeInstallOutcome;
pub use view::{
    CatalogState, InstallPhase, InstallProgress, InstalledSummary, PluginOrigin, StoreDetail,
    StoreEntry, StoreOverview,
};

use catalog::{Detail, Listing};
use view::{OverviewInput, PluginPresence, ThemePresence, build_overview};

const EVENT_CHANNEL_CAPACITY: usize = 16;

/// Why a store operation failed. Every message names the next step.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Fetch(#[from] FetchError),
    #[error("{0}")]
    Catalog(#[from] CatalogError),
    #[error("no catalog is available yet; refresh the store while online")]
    NoCatalog,
    #[error("\"{id}\" is not listed in the catalog; refresh the store")]
    Unknown { id: String },
    #[error("{what} of \"{id}\" does not match its listed SHA-256; refresh and try again")]
    DigestMismatch { what: &'static str, id: String },
    #[error("SMABAR_STORE_ENDPOINT is not a usable base URL: {0}")]
    Endpoint(String),
    #[error("the store could not be reached ({0}); installing needs a fresh catalog — try again")]
    StoreUnreachable(String),
    #[error("another install is running; wait for it to finish")]
    Busy,
    #[error("\"{id}\" is not a valid id; ids contain only [a-z0-9-]")]
    InvalidId { id: String },
    #[error(
        "the listing of \"{id}\" changed since you looked (expected {expected}, now {actual}); refresh and try again"
    )]
    VersionChanged {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("\"{id}\" ships with smabar; it cannot be installed from the store")]
    BasePlugin { id: String },
    #[error(
        "\"{id}\" exists as your own plugin in {} without a store receipt; the store never replaces it — remove it first",
        path.display()
    )]
    UserPlugin { id: String, path: PathBuf },
    #[error("\"{id}\" {version} is blocked by the store: {reason}")]
    Blocked {
        id: String,
        version: String,
        reason: String,
    },
    #[error("\"{id}\" needs {requirement}; this smabar is {app_version} on {host_os}")]
    Incompatible {
        id: String,
        requirement: String,
        app_version: String,
        host_os: String,
    },
    #[error(
        "\"{id}\" was edited locally since the store installed it; confirm to replace it (a backup is kept in {})",
        backup.display()
    )]
    LocallyModified { id: String, backup: PathBuf },
    #[error(
        "the listing of \"{id}\" violates the catalog contract ({detail}); refresh, and report this if it persists"
    )]
    Contract { id: String, detail: String },
    #[error(
        "the downloaded archive of \"{id}\" does not match the listed tree ({expected} vs {actual}); \
         the repository may use export-ignore or eol attributes — nothing was installed"
    )]
    TreeMismatch {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("{0}")]
    Archive(#[from] archive::ArchiveError),
    #[error("the downloaded smabar.json is invalid: {0}")]
    Manifest(#[from] crate::plugins::ManifestError),
    #[error("the downloaded manifest declares id \"{actual}\", the listing says \"{expected}\"")]
    IdMismatch { expected: String, actual: String },
    #[error(
        "the downloaded manifest of \"{id}\" says version {manifest}, the listing says {listed}"
    )]
    VersionMismatch {
        id: String,
        listed: String,
        manifest: String,
    },
    #[error(
        "\"{id}\" names the entry script \"{entry}\", which is not a file inside the plugin folder"
    )]
    EntryMissing { id: String, entry: String },
    #[error("{0}")]
    Replace(#[from] crate::plugins::ReplaceError),
    #[error("\"{id}\" failed to start after the install ({reason}); {}", if *rolled_back { "the previous state was restored" } else { "check the backup folder" })]
    StartFailed {
        id: String,
        reason: String,
        rolled_back: bool,
    },
    #[error("{0}")]
    Theme(#[from] crate::themes::io::ThemeIoError),
    #[error("\"{name}\" is a theme compiled into smabar; it cannot be installed from the store")]
    BundledTheme { name: String },
    #[error(
        "a theme named \"{name}\" exists in {} without a store receipt; the store never replaces it — delete it first",
        path.display()
    )]
    LocalTheme { name: String, path: PathBuf },
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Fanned out to every frontend when the overview changed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreEvent {
    pub reason: ChangeReason,
    pub catalog_state: CatalogState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeReason {
    Refresh,
    Install,
    Remove,
}

/// The catalog held in memory, plus what the last refresh learned.
pub(crate) struct StoreState {
    pub(crate) listing: Option<Listing>,
    /// SHA-256 of the verified catalog bytes, to spot an unchanged body when
    /// the CDN rewrote the ETag.
    pub(crate) body_sha256: Option<String>,
    pub(crate) etag: Option<String>,
    pub(crate) fetched_at: Option<u64>,
    pub(crate) catalog_state: CatalogState,
    pub(crate) last_error: Option<String>,
    pub(crate) pending: Option<InstallProgress>,
    /// Verified detail files, keyed by their listed SHA-256.
    pub(crate) details: BTreeMap<String, Detail>,
}

pub(crate) struct Inner {
    pub(crate) paths: SmabarPaths,
    pub(crate) config: Arc<ConfigWatcher>,
    pub(crate) supervisor: PluginSupervisor,
    pub(crate) fetcher: Arc<dyn StoreFetcher>,
    pub(crate) endpoint: Url,
    pub(crate) key: PublicKey,
    pub(crate) app_version: Version,
    pub(crate) host_os: Option<&'static str>,
    pub(crate) reserved_ids: BTreeSet<String>,
    pub(crate) state: Mutex<StoreState>,
    /// Serializes catalog fetches (UI button and background timer).
    pub(crate) refresh_lock: tokio::sync::Mutex<()>,
    /// One install, update or removal at a time.
    pub(crate) install_lock: tokio::sync::Mutex<()>,
    /// Read-modify-write cycles of `installed.json`.
    pub(crate) receipts_lock: tokio::sync::Mutex<()>,
    pub(crate) events: broadcast::Sender<StoreEvent>,
}

/// What the app edge decides for the store client.
pub struct StoreOptions {
    /// Base URL of the store, ending in `/`.
    pub endpoint: Url,
    /// The catalog signing key; normally [`catalog::embedded_key`].
    pub key: PublicKey,
    /// This smabar's version, compared with `requires.smabar`.
    pub app_version: String,
    /// Ids of the plugins that ship with smabar; never installed over.
    pub reserved_ids: BTreeSet<String>,
}

/// The one store client of the process; cloning shares it.
#[derive(Clone)]
pub struct StoreService {
    pub(crate) inner: Arc<Inner>,
}

impl StoreService {
    /// Loads the cached catalog (only if its signature still verifies) and
    /// is ready to serve; no network is touched until [`Self::refresh`].
    pub fn new(
        paths: SmabarPaths,
        config: Arc<ConfigWatcher>,
        supervisor: PluginSupervisor,
        fetcher: Arc<dyn StoreFetcher>,
        options: StoreOptions,
    ) -> Result<Self, StoreError> {
        let StoreOptions {
            endpoint,
            key,
            app_version,
            reserved_ids,
        } = options;
        let app_version = Version::parse(&app_version)
            .map_err(|error| StoreError::Endpoint(format!("app version is not SemVer: {error}")))?;
        if !endpoint.path().ends_with('/') {
            return Err(StoreError::Endpoint(format!(
                "{endpoint} must end with a slash"
            )));
        }
        let cached = refresh::load_cached(&paths, &key);
        let state = StoreState {
            catalog_state: if cached.is_some() {
                CatalogState::Stale
            } else {
                CatalogState::Unavailable
            },
            listing: cached.as_ref().map(|cached| cached.listing.clone()),
            body_sha256: cached.as_ref().map(|cached| cached.body_sha256.clone()),
            etag: cached.as_ref().and_then(|cached| cached.etag.clone()),
            fetched_at: cached.as_ref().and_then(|cached| cached.fetched_at),
            last_error: None,
            pending: None,
            details: BTreeMap::new(),
        };
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Ok(Self {
            inner: Arc::new(Inner {
                paths,
                config,
                supervisor,
                fetcher,
                endpoint,
                key,
                app_version,
                host_os: crate::platform::current_os(),
                reserved_ids,
                state: Mutex::new(state),
                refresh_lock: tokio::sync::Mutex::new(()),
                install_lock: tokio::sync::Mutex::new(()),
                receipts_lock: tokio::sync::Mutex::new(()),
                events,
            }),
        })
    }

    /// Notifications that the overview changed.
    pub fn subscribe(&self) -> broadcast::Receiver<StoreEvent> {
        self.inner.events.subscribe()
    }

    /// The catalog joined with what is installed, from memory and disk; no
    /// network.
    pub fn overview(&self) -> StoreOverview {
        overview(&self.inner)
    }

    /// Fetches the catalog (conditionally), verifies it, applies the
    /// blocklist and returns the new overview. Never fails: an unreachable
    /// store leaves the previous catalog in place and is reported through
    /// `catalogState` and `lastError`.
    pub async fn refresh(&self) -> StoreOverview {
        refresh::refresh(&self.inner).await
    }

    /// One item's detail file (README, releases), verified against the
    /// SHA-256 the listing carries.
    pub async fn detail(&self, kind: ItemKind, id: &str) -> Result<StoreDetail, StoreError> {
        refresh::detail(&self.inner, kind, id).await
    }

    /// Installs or updates a plugin to the listed version, after a fresh
    /// refresh. `expected_version` is the version the user saw; a listing
    /// that moved since is refused. `confirm_modified` allows replacing a
    /// locally edited copy (a backup is kept either way).
    pub async fn install_plugin(
        &self,
        id: &str,
        expected_version: &str,
        confirm_modified: bool,
        progress: ProgressSink<'_>,
    ) -> Result<InstallOutcome, StoreError> {
        install::install_plugin(
            &self.inner,
            id,
            expected_version,
            confirm_modified,
            progress,
        )
        .await
    }

    /// Installs or updates a theme to the listed version.
    pub async fn install_theme(
        &self,
        name: &str,
        expected_version: &str,
    ) -> Result<ThemeInstallOutcome, StoreError> {
        themes::install_theme(&self.inner, name, expected_version).await
    }

    /// Tells subscribers that something was removed through the plugin or
    /// theme removal paths, which do not know the store.
    pub fn note_removed(&self) {
        notify(&self.inner, ChangeReason::Remove);
    }

    /// Origin, version and update state of every installed plugin, keyed by
    /// id — the one source the Installed list and the store group share.
    pub fn installed_summaries(&self) -> BTreeMap<String, InstalledSummary> {
        let overview = overview(&self.inner);
        let receipts = receipts::load(&self.inner.paths);
        self.inner
            .supervisor
            .plugin_infos()
            .into_iter()
            .map(|info| {
                let receipt = receipts.plugins.get(&info.id);
                let origin = if self.inner.reserved_ids.contains(&info.id) {
                    PluginOrigin::Base
                } else if receipt.is_some() {
                    PluginOrigin::Community
                } else {
                    PluginOrigin::User
                };
                let update = overview
                    .entries
                    .iter()
                    .find(|entry| entry.kind == ItemKind::Plugin && entry.id == info.id)
                    .and_then(|entry| entry.update.as_ref())
                    .map(|update| update.to_version.clone());
                let summary = InstalledSummary {
                    origin,
                    version: info
                        .manifest
                        .as_ref()
                        .map(|manifest| manifest.version.clone()),
                    update,
                    modified: receipt.is_some_and(|receipt| {
                        receipts::is_plugin_modified(&self.inner.paths, &info.id, receipt)
                    }),
                    blocked: receipt
                        .and_then(|receipt| receipt.blocked.as_ref())
                        .map(|blocked| blocked.reason.clone()),
                };
                (info.id, summary)
            })
            .collect()
    }
}

/// Joins catalog and disk. The digests of installed Community Plugins are
/// recomputed on every call; that is a handful of small folders.
pub(crate) fn overview(inner: &Inner) -> StoreOverview {
    let receipts = receipts::load(&inner.paths);
    let infos = inner.supervisor.plugin_infos();
    // The config is the intent; the supervisor's status follows it a debounce
    // later, which is exactly the window in which the blocklist just wrote it.
    let deactivated = inner.config.current().plugins_deactivated;
    let mut plugins = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(inner.paths.plugins_dir()) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let info = infos.iter().find(|info| info.id == id);
            let modified = receipts
                .plugins
                .get(&id)
                .map(|receipt| receipts::is_plugin_modified(&inner.paths, &id, receipt));
            plugins.insert(
                id.clone(),
                PluginPresence {
                    version: info
                        .and_then(|info| info.manifest.as_ref())
                        .map(|manifest| manifest.version.clone()),
                    deactivated: deactivated.contains(&id),
                    modified,
                },
            );
        }
    }
    let mut themes = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(inner.paths.themes_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let modified = receipts
                .themes
                .get(name)
                .map(|receipt| receipts::is_theme_modified(&inner.paths, name, receipt));
            themes.insert(name.to_string(), ThemePresence { modified });
        }
    }
    let state = lock_unpoisoned(&inner.state);
    build_overview(OverviewInput {
        listing: state.listing.as_ref(),
        catalog_state: state.catalog_state,
        fetched_at: state.fetched_at,
        last_error: state.last_error.clone(),
        receipts: &receipts,
        plugins: &plugins,
        themes: &themes,
        reserved_ids: &inner.reserved_ids,
        app_version: &inner.app_version,
        host_os: inner.host_os,
        pending: state.pending.clone(),
    })
}

pub(crate) fn notify(inner: &Inner, reason: ChangeReason) {
    let catalog_state = lock_unpoisoned(&inner.state).catalog_state;
    if inner
        .events
        .send(StoreEvent {
            reason,
            catalog_state,
        })
        .is_err()
    {
        tracing::debug!("store change had no subscribers");
    }
}
