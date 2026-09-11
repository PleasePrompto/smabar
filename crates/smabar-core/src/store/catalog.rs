//! The Community Catalog as `store.smabar.com` serves it: the wire models
//! and the signature check that gates every byte before it is parsed.
//!
//! The catalog is a contract (`catalog-v1.json`): new fields are optional and
//! existing ones keep their meaning, so nothing here uses
//! `deny_unknown_fields` — a client must keep working when the store adds a
//! field.

use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::plugins::PluginRuntime;

/// The store's minisign public key. It belongs to the store service, not to
/// the app updater; a catalog signed by any other key is rejected.
pub const CATALOG_PUBLIC_KEY_B64: &str = "RWQ016+I83QhYvasmFfLn4TSSErMRzLnjYVnxvd4bsAZz1kxDUbbLMTX";
/// The one listing schema this client reads; a breaking change ships as
/// `catalog-v2.json` beside it.
pub const CATALOG_SCHEMA: u32 = 1;

/// Why catalog bytes were refused.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// The signature does not match the bytes — tampered in transit, or
    /// signed by a key that is not the store's.
    #[error("catalog signature rejected ({0}); the catalog was altered or signed by another key")]
    Signature(String),
    /// Verified bytes that are not the JSON this client expects.
    #[error("catalog is not valid JSON: {0}")]
    Parse(#[source] serde_json::Error),
    /// A newer, incompatible listing schema.
    #[error("catalog schema {0} is not supported (this smabar reads schema {CATALOG_SCHEMA})")]
    Schema(u32),
    /// The compiled-in key line is malformed — a build error, surfaced
    /// instead of panicking.
    #[error("the embedded store key is malformed: {0}")]
    Key(String),
}

/// The store key as a verifier.
pub fn embedded_key() -> Result<PublicKey, CatalogError> {
    PublicKey::from_base64(CATALOG_PUBLIC_KEY_B64)
        .map_err(|error| CatalogError::Key(error.to_string()))
}

/// Verifies the raw catalog bytes against `signature` and only then parses
/// them. Legacy (non-prehashed) signatures are refused: the store never
/// produces them, so one would be a forgery attempt.
pub fn verify_and_parse(
    bytes: &[u8],
    signature: &str,
    key: &PublicKey,
) -> Result<Listing, CatalogError> {
    let signature =
        Signature::decode(signature).map_err(|error| CatalogError::Signature(error.to_string()))?;
    key.verify(bytes, &signature, false)
        .map_err(|error| CatalogError::Signature(error.to_string()))?;
    let listing: Listing = serde_json::from_slice(bytes).map_err(CatalogError::Parse)?;
    if listing.schema != CATALOG_SCHEMA {
        return Err(CatalogError::Schema(listing.schema));
    }
    Ok(listing)
}

/// Lowercase hex SHA-256, the digest format of every hash the catalog lists.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn default_schema() -> u32 {
    CATALOG_SCHEMA
}

fn all_operating_systems() -> Vec<String> {
    vec![
        "linux".to_string(),
        "windows".to_string(),
        "macos".to_string(),
    ]
}

/// What a catalog entry is: a plugin folder or a theme file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Plugin,
    Theme,
}

/// `catalog-v1.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// ISO-8601 UTC; moves only when the content changed.
    pub generated_at: String,
    #[serde(default)]
    pub items: Vec<Entry>,
    #[serde(default)]
    pub blocklist: Vec<BlockEntry>,
}

/// One listed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Entry {
    Plugin(PluginEntry),
    Theme(ThemeEntry),
}

impl Entry {
    /// The fields every kind shares.
    pub fn common(&self) -> &EntryCommon {
        match self {
            Self::Plugin(plugin) => &plugin.common,
            Self::Theme(theme) => &theme.common,
        }
    }

    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Plugin(_) => ItemKind::Plugin,
            Self::Theme(_) => ItemKind::Theme,
        }
    }

    /// The commit the item is served from.
    pub fn commit(&self) -> &str {
        match self {
            Self::Plugin(plugin) => &plugin.source.commit,
            Self::Theme(theme) => &theme.source.commit,
        }
    }

    /// The tag or branch that commit came from.
    pub fn git_ref(&self) -> &str {
        match self {
            Self::Plugin(plugin) => &plugin.source.git_ref,
            Self::Theme(theme) => &theme.source.git_ref,
        }
    }
}

/// Fields shared by plugin and theme entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryCommon {
    pub id: String,
    pub name: String,
    /// SemVer; the update unit.
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub requires: Requires,
    pub author: Author,
    pub repo: Repository,
    /// `.` for a root plugin, `plugins/<id>` in a collection,
    /// `themes/<name>.json` for a theme.
    pub path: String,
    pub updated_at: String,
    /// SHA-256 of the exact bytes of `/items/<kind>/<id>.json`.
    pub detail_sha256: String,
}

/// A plugin listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    #[serde(flatten)]
    pub common: EntryCommon,
    pub runtime: PluginRuntime,
    pub source: PluginSource,
}

/// A theme listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeEntry {
    #[serde(flatten)]
    pub common: EntryCommon,
    pub source: ThemeSource,
}

/// What an item needs from the machine it is installed on.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requires {
    /// Minimum smabar version (SemVer), if the author declared one.
    #[serde(default)]
    pub smabar: Option<String>,
    #[serde(default = "all_operating_systems")]
    pub os: Vec<String>,
    /// Programs the user has to install themselves; never installed by smabar.
    #[serde(default)]
    pub external: Vec<String>,
}

impl Default for Requires {
    fn default() -> Self {
        Self {
            smabar: None,
            os: all_operating_systems(),
            external: Vec::new(),
        }
    }
}

/// The GitHub account that owns the repository.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub login: String,
    pub url: String,
}

/// The source repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    /// GitHub's database id — the identity the store binds ids to.
    pub id: i64,
    pub url: String,
    pub name_with_owner: String,
    /// SPDX id, or `None` when GitHub could not tell.
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub stars: i64,
    #[serde(default)]
    pub open_issues: i64,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

/// Where a plugin's bytes come from and what they must hash to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSource {
    pub commit: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// GitHub's zip archive of `commit`; the plugin folder inside is
    /// `<repo>-<commit>/<path>`.
    pub archive_url: String,
    /// Git tree oid of the plugin folder at `commit`.
    pub tree_oid: String,
    /// SHA-256 of `smabar.json` at `commit`.
    pub manifest_sha256: String,
}

/// Where a theme's file comes from and what it must hash to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSource {
    pub commit: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub file_url: String,
    pub sha256: String,
}

/// The store's kill switch for one id, or one version of it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockEntry {
    pub kind: ItemKind,
    pub id: String,
    pub reason: String,
    /// `None` blocks every version.
    #[serde(default)]
    pub version: Option<String>,
}

/// `/items/<kind>/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    #[serde(default = "default_schema")]
    pub schema: u32,
    pub kind: ItemKind,
    pub id: String,
    pub version: String,
    /// README markdown as published, at most 20 000 characters; rendered by
    /// `store::readme` before it reaches the shell.
    #[serde(default)]
    pub readme: Option<String>,
    #[serde(default)]
    pub releases: Vec<Release>,
}

/// One published release tag that matched the item.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub commit: String,
    #[serde(default)]
    pub published_at: Option<String>,
}

impl Listing {
    /// The block that applies to `version` of `id`: one naming that version,
    /// or one blocking every version.
    pub fn block_entry(&self, kind: ItemKind, id: &str, version: &str) -> Option<&BlockEntry> {
        self.blocklist.iter().find(|entry| {
            entry.kind == kind
                && entry.id == id
                && entry
                    .version
                    .as_deref()
                    .is_none_or(|blocked| blocked == version)
        })
    }

    /// The block reason for `version` of `id`, if the store blocked it (or
    /// every version of it).
    pub fn block_reason(&self, kind: ItemKind, id: &str, version: &str) -> Option<&str> {
        self.block_entry(kind, id, version)
            .map(|entry| entry.reason.as_str())
    }

    pub fn plugin(&self, id: &str) -> Option<&PluginEntry> {
        self.items.iter().find_map(|entry| match entry {
            Entry::Plugin(plugin) if plugin.common.id == id => Some(plugin),
            _ => None,
        })
    }

    pub fn theme(&self, name: &str) -> Option<&ThemeEntry> {
        self.items.iter().find_map(|entry| match entry {
            Entry::Theme(theme) if theme.common.id == name => Some(theme),
            _ => None,
        })
    }
}
