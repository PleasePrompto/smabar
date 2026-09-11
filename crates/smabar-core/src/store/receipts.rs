//! Install receipts: smabar's own record of what the Community Store put
//! into `plugins/` and `themes/`.
//!
//! The receipt is what separates a Community Plugin from a User Plugin. A
//! folder without one is the user's own — the store never replaces it, and
//! updates are offered only for folders it installed itself. The content
//! digest recorded at install time is how a local edit is noticed later
//! (ADR 0002: modified code is warned about and backed up, never overwritten
//! on a guess).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::SmabarPaths;
use crate::util::write_atomically;

const RECEIPTS_SCHEMA: u32 = 1;

fn receipts_schema() -> u32 {
    RECEIPTS_SCHEMA
}

/// `store/installed.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipts {
    #[serde(default = "receipts_schema")]
    pub schema: u32,
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginReceipt>,
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeReceipt>,
}

/// Why an installed item is switched off, remembered until the catalog that
/// blocked it is superseded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedState {
    pub reason: String,
    /// The blocked version, `None` when every version is.
    #[serde(default)]
    pub version: Option<String>,
    /// `generatedAt` of the catalog that carried the block; only a NEWER
    /// catalog without it lifts the block (replay protection).
    pub catalog_generated_at: String,
}

/// One installed Community Plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginReceipt {
    pub repo_id: i64,
    pub name_with_owner: String,
    pub path: String,
    pub version: String,
    pub commit: String,
    pub tree_oid: String,
    /// [`folder_digest`] of the folder as installed; differs once the user
    /// edits a file.
    pub installed_digest: String,
    pub installed_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<BlockedState>,
    /// The store switched this plugin off (blocklist), so the store may
    /// switch it back on when the block is lifted. A user's own deactivation
    /// is never undone.
    #[serde(default)]
    pub deactivated_by_store: bool,
}

/// One installed Community Theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeReceipt {
    pub repo_id: i64,
    pub name_with_owner: String,
    pub path: String,
    pub version: String,
    pub commit: String,
    /// SHA-256 the catalog listed for the source file.
    pub source_sha256: String,
    /// SHA-256 of the file as written into `themes/` (canonical form).
    pub written_sha256: String,
    pub installed_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<BlockedState>,
}

/// Reads the receipts; a missing or unreadable file is an empty set, so
/// losing it turns Community Plugins into User Plugins — never into data
/// loss.
pub fn load(paths: &SmabarPaths) -> Receipts {
    match load_checked(paths) {
        Ok(receipts) => receipts,
        Err(error) => {
            tracing::warn!(
                path = %paths.store_receipts_file().display(),
                %error,
                "ignoring unreadable store receipts; installed community plugins count as the user's own until reinstalled"
            );
            Receipts::default()
        }
    }
}

/// Transactions must not overwrite unreadable receipts with an empty set.
pub(crate) fn load_checked(paths: &SmabarPaths) -> io::Result<Receipts> {
    let raw = match fs::read(paths.store_receipts_file()) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Receipts::default()),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&raw).map_err(io::Error::other)
}

/// Persists the receipts atomically.
pub fn save(paths: &SmabarPaths, receipts: &Receipts) -> io::Result<()> {
    let mut json = serde_json::to_vec_pretty(receipts).map_err(io::Error::other)?;
    json.push(b'\n');
    write_atomically(&paths.store_receipts_file(), &json)
}

/// Drops a plugin's receipt and backup. Filesystem only, so the plugin
/// removal path can call it without touching the supervisor. Pending recovery
/// returns `false` without deleting anything; otherwise reports whether a receipt existed.
pub fn forget_plugin(paths: &SmabarPaths, plugin_id: &str) -> bool {
    if super::journal::pending(paths) {
        tracing::warn!(
            plugin_id,
            "store recovery is pending; retaining the receipt and backup until recovery succeeds"
        );
        return false;
    }
    let backup = paths.store_backup_dir(plugin_id);
    if backup.is_dir()
        && let Err(error) = fs::remove_dir_all(&backup)
    {
        tracing::warn!(path = %backup.display(), %error, "could not remove a store backup");
    }
    let mut receipts = load(paths);
    if receipts.plugins.remove(plugin_id).is_none() {
        return false;
    }
    if let Err(error) = save(paths, &receipts) {
        tracing::warn!(
            plugin_id,
            %error,
            "removed the plugin but could not drop its store receipt; delete the entry from store/installed.json by hand"
        );
    }
    true
}

/// Drops a theme's receipt. Returns whether one existed.
pub fn forget_theme(paths: &SmabarPaths, name: &str) -> bool {
    let mut receipts = load(paths);
    if receipts.themes.remove(name).is_none() {
        return false;
    }
    if let Err(error) = save(paths, &receipts) {
        tracing::warn!(theme = name, %error, "could not drop a theme's store receipt");
    }
    true
}

/// SHA-256 over a folder's files (sorted relative paths and bytes), skipping
/// the build artifacts a running plugin creates (`__pycache__`, dot entries)
/// and ignoring file modes, which Windows cannot keep.
///
/// Deliberately NOT the git tree oid: that one covers dotfiles and modes, so
/// a plugin that merely ran would look modified.
pub fn folder_digest(dir: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect(dir, PathBuf::new(), &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        let bytes = fs::read(dir.join(&relative))?;
        let name = relative
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect(root: &Path, prefix: PathBuf, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(&prefix))? {
        let entry = entry?;
        if crate::plugins::is_build_artifact(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let relative = prefix.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            collect(root, relative, out)?;
        } else {
            out.push(relative);
        }
    }
    Ok(())
}

/// Whether the installed folder no longer matches its receipt. An unreadable
/// folder counts as unmodified: the install path re-checks before it
/// replaces anything.
pub fn is_plugin_modified(paths: &SmabarPaths, plugin_id: &str, receipt: &PluginReceipt) -> bool {
    match folder_digest(&paths.plugins_dir().join(plugin_id)) {
        Ok(digest) => digest != receipt.installed_digest,
        Err(error) => {
            tracing::debug!(plugin_id, %error, "cannot digest the installed plugin folder");
            false
        }
    }
}

/// Whether the theme file no longer matches its receipt.
pub fn is_theme_modified(paths: &SmabarPaths, name: &str, receipt: &ThemeReceipt) -> bool {
    match fs::read(paths.themes_dir().join(format!("{name}.json"))) {
        Ok(bytes) => super::catalog::sha256_hex(&bytes) != receipt.written_sha256,
        Err(_) => false,
    }
}
