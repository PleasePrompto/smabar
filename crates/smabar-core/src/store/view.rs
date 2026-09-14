//! What the settings UI sees: the catalog joined with what is installed.
//!
//! [`build_overview`] is pure — every fact about the filesystem arrives as an
//! argument — so the whole decision table (installed, update, blocked,
//! incompatible, installable) is unit-tested without a store or a disk.

use std::collections::{BTreeMap, BTreeSet};

use semver::Version;
use serde::Serialize;

use crate::plugins::PluginRuntime;

use super::catalog::{
    Author, BlockEntry, Entry, EntryCommon, ItemKind, Listing, Release, Requires,
};
use super::compat::{self, Incompatibility};
use super::receipts::{BlockedState, Receipts};

/// How current the catalog behind an overview is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CatalogState {
    /// Fetched (or confirmed unchanged) during this session.
    Fresh,
    /// Served from the verified cache because the store did not answer.
    Stale,
    /// No verified catalog at all.
    Unavailable,
}

/// The one in-flight install, as the UI shows it.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub kind: ItemKind,
    pub id: String,
    pub phase: InstallPhase,
    pub received: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum InstallPhase {
    Downloading,
    Verifying,
    Installing,
    Starting,
    Done,
}

/// The catalog as presented, plus the state of the catalog itself.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreOverview {
    pub catalog_state: CatalogState,
    pub generated_at: Option<String>,
    /// Unix milliseconds of the last successful fetch.
    pub fetched_at: Option<u64>,
    pub last_error: Option<String>,
    pub app_version: String,
    pub host_os: Option<String>,
    pub entries: Vec<StoreEntry>,
    pub pending: Option<InstallProgress>,
}

/// One listed item with everything the UI decides on.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreEntry {
    pub kind: ItemKind,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub author: Author,
    pub repo: RepoView,
    pub runtime: Option<PluginRuntime>,
    pub requires: Requires,
    pub path: String,
    pub commit: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub updated_at: String,
    pub installed: Option<InstalledView>,
    pub update: Option<UpdateView>,
    /// The LISTED version is blocked by the store.
    pub blocked: Option<BlockView>,
    pub incompatible: Vec<Incompatibility>,
    /// Install or update is possible here right now.
    pub installable: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub url: String,
    pub name_with_owner: String,
    pub license: Option<String>,
    pub stars: i64,
    pub archived: bool,
    pub pushed_at: Option<String>,
}

/// Who put the installed copy there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Installed by the store; has a receipt.
    Store,
    /// A folder or file the user placed; the store never replaces it.
    Local,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledView {
    pub version: String,
    pub commit: String,
    pub origin: Origin,
    pub modified: bool,
    pub deactivated: bool,
    /// The store switched this installed version off.
    pub blocked: Option<BlockView>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockView {
    pub reason: String,
    pub version: Option<String>,
}

impl From<&BlockEntry> for BlockView {
    fn from(entry: &BlockEntry) -> Self {
        Self {
            reason: entry.reason.clone(),
            version: entry.version.clone(),
        }
    }
}

impl From<&BlockedState> for BlockView {
    fn from(state: &BlockedState) -> Self {
        Self {
            reason: state.reason.clone(),
            version: state.version.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    pub from_version: String,
    pub to_version: String,
    /// Same version, different content (the author pushed without a bump).
    pub content_changed: bool,
}

/// Who put an installed plugin there, as the Installed list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PluginOrigin {
    /// Ships with smabar.
    Base,
    /// Installed from the Community Store (has a receipt).
    Community,
    /// The user's own.
    User,
}

/// What the Installed list needs about one plugin, whatever its origin.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSummary {
    pub origin: PluginOrigin,
    pub version: Option<String>,
    /// A newer catalog version, when the store offers one.
    pub update: Option<String>,
    pub modified: bool,
    /// The store's block reason, when the store switched it off.
    pub blocked: Option<String>,
}

/// One item with its detail file.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreDetail {
    pub entry: StoreEntry,
    /// The README as published (markdown), for agents.
    pub readme: Option<String>,
    /// `readme` rendered by smabar's own converter: raw HTML dropped, links
    /// absolute and opened outside, images only from GitHub hosts.
    pub readme_html: Option<String>,
    pub releases: Vec<Release>,
}

/// What exists on disk for one plugin id.
#[derive(Debug, Clone, Default)]
pub struct PluginPresence {
    /// `version` of the folder's manifest, when it parsed.
    pub version: Option<String>,
    pub deactivated: bool,
    /// Whether the folder differs from its receipt; `None` without a receipt.
    pub modified: Option<bool>,
}

/// What exists on disk for one theme name.
#[derive(Debug, Clone, Default)]
pub struct ThemePresence {
    pub modified: Option<bool>,
}

/// Everything [`build_overview`] joins.
pub struct OverviewInput<'a> {
    pub listing: Option<&'a Listing>,
    pub catalog_state: CatalogState,
    pub fetched_at: Option<u64>,
    pub last_error: Option<String>,
    pub receipts: &'a Receipts,
    /// Every plugin folder under `plugins/`, by id.
    pub plugins: &'a BTreeMap<String, PluginPresence>,
    /// Every drop-in theme under `themes/`, by name.
    pub themes: &'a BTreeMap<String, ThemePresence>,
    /// Ids of the plugins that ship with smabar.
    pub reserved_ids: &'a BTreeSet<String>,
    pub app_version: &'a Version,
    pub host_os: Option<&'a str>,
    pub pending: Option<InstallProgress>,
}

/// Joins the catalog with the installed state. Pure.
pub fn build_overview(input: OverviewInput<'_>) -> StoreOverview {
    let entries = input
        .listing
        .map(|listing| {
            listing
                .items
                .iter()
                .map(|entry| view_entry(entry, listing, &input))
                .collect()
        })
        .unwrap_or_default();
    StoreOverview {
        catalog_state: input.catalog_state,
        generated_at: input.listing.map(|listing| listing.generated_at.clone()),
        fetched_at: input.fetched_at,
        last_error: input.last_error,
        app_version: input.app_version.to_string(),
        host_os: input.host_os.map(str::to_string),
        entries,
        pending: input.pending,
    }
}

fn view_entry(entry: &Entry, listing: &Listing, input: &OverviewInput<'_>) -> StoreEntry {
    let common = entry.common();
    let kind = entry.kind();
    let listed_block = listing
        .block_entry(kind, &common.id, &common.version)
        .map(BlockView::from);
    let mut incompatible = Vec::new();
    let (installed, update) = match entry {
        Entry::Plugin(plugin) => {
            if !compat::os_allowed(&common.requires.os, input.host_os) {
                incompatible.push(Incompatibility::Os);
            }
            if !compat::smabar_satisfies(input.app_version, common.requires.smabar.as_deref()) {
                incompatible.push(Incompatibility::MinSmabar);
            }
            if input.reserved_ids.contains(&common.id) {
                incompatible.push(Incompatibility::BasePlugin);
            }
            let receipt = input.receipts.plugins.get(&common.id);
            let presence = input.plugins.get(&common.id);
            let installed = presence.map(|presence| match receipt {
                Some(receipt) => InstalledView {
                    version: receipt.version.clone(),
                    commit: receipt.commit.clone(),
                    origin: Origin::Store,
                    modified: presence.modified.unwrap_or(false),
                    deactivated: presence.deactivated,
                    blocked: receipt.blocked.as_ref().map(BlockView::from),
                },
                None => InstalledView {
                    version: presence.version.clone().unwrap_or_default(),
                    commit: String::new(),
                    origin: Origin::Local,
                    modified: false,
                    deactivated: presence.deactivated,
                    blocked: None,
                },
            });
            if presence.is_some() && receipt.is_none() {
                incompatible.push(Incompatibility::UserPlugin);
            }
            let update = presence.and(receipt).and_then(|receipt| {
                update_view(
                    &receipt.version,
                    &common.version,
                    &receipt.tree_oid,
                    &plugin.source.tree_oid,
                )
            });
            (installed, update)
        }
        Entry::Theme(theme) => {
            if crate::themes::is_bundled(&common.id) {
                incompatible.push(Incompatibility::BundledTheme);
            }
            let receipt = input.receipts.themes.get(&common.id);
            let presence = input.themes.get(&common.id);
            let installed = presence.map(|presence| match receipt {
                Some(receipt) => InstalledView {
                    version: receipt.version.clone(),
                    commit: receipt.commit.clone(),
                    origin: Origin::Store,
                    modified: presence.modified.unwrap_or(false),
                    deactivated: false,
                    blocked: receipt.blocked.as_ref().map(BlockView::from),
                },
                None => InstalledView {
                    version: String::new(),
                    commit: String::new(),
                    origin: Origin::Local,
                    modified: false,
                    deactivated: false,
                    blocked: None,
                },
            });
            if presence.is_some() && receipt.is_none() {
                incompatible.push(Incompatibility::UserPlugin);
            }
            let update = presence.and(receipt).and_then(|receipt| {
                update_view(
                    &receipt.version,
                    &common.version,
                    &receipt.source_sha256,
                    &theme.source.sha256,
                )
            });
            (installed, update)
        }
    };
    if listed_block.is_some() {
        incompatible.push(Incompatibility::Blocked);
    }
    let installable = incompatible.is_empty() && (installed.is_none() || update.is_some());
    StoreEntry {
        kind,
        id: common.id.clone(),
        name: common.name.clone(),
        version: common.version.clone(),
        description: common.description.clone(),
        keywords: common.keywords.clone(),
        author: common.author.clone(),
        repo: repo_view(common),
        runtime: match entry {
            Entry::Plugin(plugin) => Some(plugin.runtime),
            Entry::Theme(_) => None,
        },
        requires: common.requires.clone(),
        path: common.path.clone(),
        commit: entry.commit().to_string(),
        git_ref: entry.git_ref().to_string(),
        updated_at: common.updated_at.clone(),
        installed,
        update,
        blocked: listed_block,
        incompatible,
        installable,
    }
}

pub(super) fn update_view(
    installed_version: &str,
    listed_version: &str,
    installed_hash: &str,
    listed_hash: &str,
) -> Option<UpdateView> {
    if compat::is_newer(listed_version, installed_version) {
        return Some(UpdateView {
            from_version: installed_version.to_string(),
            to_version: listed_version.to_string(),
            content_changed: false,
        });
    }
    (installed_version == listed_version && installed_hash != listed_hash).then(|| UpdateView {
        from_version: installed_version.to_string(),
        to_version: listed_version.to_string(),
        content_changed: true,
    })
}

fn repo_view(common: &EntryCommon) -> RepoView {
    RepoView {
        url: common.repo.url.clone(),
        name_with_owner: common.repo.name_with_owner.clone(),
        license: common.repo.license.clone(),
        stars: common.repo.stars,
        archived: common.repo.archived,
        pushed_at: common.repo.pushed_at.clone(),
    }
}
