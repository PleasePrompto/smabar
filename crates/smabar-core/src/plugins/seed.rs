//! Installation and managed updates of the bundled base plugins.
//!
//! The hard part is not copying files, it is telling a NEW BUNDLED VERSION
//! apart from a USER EDIT. Both look like "the folder differs from the
//! resources", and the two deserve opposite treatment (ADR 0002: user plugins
//! are never replaced, modified base plugins are only replaced after asking).
//!
//! So one extra fact is recorded: the hash of what smabar itself last wrote,
//! in `cache/seed.json`. With that baseline the four cases separate cleanly —
//! see [`decide`].

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::SmabarPaths;

/// What smabar knows about one bundled plugin it installed.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedEntry {
    /// Hash of the content smabar last wrote. `None` means "no baseline" —
    /// the folder predates this bookkeeping, so it counts as modified.
    #[serde(skip_serializing_if = "Option::is_none")]
    seeded: Option<u64>,
    /// Set when the bundled version moved on but the install was edited
    /// locally. The replacement then needs a user decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    update_available: Option<u64>,
}

type SeedState = BTreeMap<String, SeedEntry>;

/// The action [`decide`] picks for one bundled plugin.
#[derive(Debug, PartialEq, Eq)]
enum SeedAction {
    /// Not installed yet.
    Install,
    /// Installed, untouched by the user, and the bundle moved on.
    Replace,
    /// Installed and edited locally while the bundle moved on: hands off.
    OfferUpdate,
    /// Up to date, a user edit, or a previously seeded plugin the user removed.
    Keep,
}

/// Decides what to do with one bundled plugin.
///
/// `installed` is `None` when the folder does not exist. `previous` is the
/// record of a previously seen bundled plugin, even if its local copy had
/// no known baseline. An absent, previously seen plugin was uninstalled.
fn decide(installed: Option<u64>, bundled: u64, previous: Option<&SeedEntry>) -> SeedAction {
    let Some(installed) = installed else {
        return if previous.is_some() {
            SeedAction::Keep
        } else {
            SeedAction::Install
        };
    };
    if installed == bundled {
        return SeedAction::Keep;
    }
    match previous.and_then(|entry| entry.seeded) {
        // Untouched since smabar wrote it, and the bundle has moved on.
        Some(baseline) if baseline == installed => SeedAction::Replace,
        // Edited locally on top of the bundle we shipped: there is nothing to
        // offer, the user simply has their own version.
        Some(baseline) if baseline == bundled => SeedAction::Keep,
        // Edited locally AND the bundle moved on, or no baseline at all —
        // never overwrite on a guess; let the update flow ask.
        _ => SeedAction::OfferUpdate,
    }
}

/// Installs bundled plugins and applies updates that need no user decision.
///
/// Plugins that are not bundled are never touched.
pub fn seed_bundled_plugins(paths: &SmabarPaths, bundled_dir: &Path) -> io::Result<()> {
    if !bundled_dir.is_dir() {
        tracing::warn!(
            path = %bundled_dir.display(),
            "bundled plugin resources are missing; base plugins were not seeded"
        );
        return Ok(());
    }

    let plugins_dir = paths.plugins_dir();
    fs::create_dir_all(&plugins_dir)?;
    let mut state = read_state(paths);
    let mut changed = false;

    for entry in fs::read_dir(bundled_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let source = entry.path();
        let plugin_id = entry.file_name().to_string_lossy().into_owned();
        let destination = plugins_dir.join(&plugin_id);

        let bundled = content_hash(&source)?;
        let installed = destination
            .is_dir()
            .then(|| content_hash(&destination))
            .transpose()?;
        let previous = state.get(&plugin_id).cloned().unwrap_or_default();
        let action = decide(installed, bundled, state.get(&plugin_id));
        let next = match action {
            SeedAction::Install | SeedAction::Replace => {
                if action == SeedAction::Replace {
                    fs::remove_dir_all(&destination)?;
                }
                install(&source, &destination)?;
                tracing::info!(
                    plugin_id = %plugin_id,
                    path = %destination.display(),
                    replaced = action == SeedAction::Replace,
                    "installed bundled plugin"
                );
                SeedEntry {
                    seeded: Some(bundled),
                    update_available: None,
                }
            }
            SeedAction::OfferUpdate => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    path = %destination.display(),
                    "bundled plugin has local changes and a newer bundled version; \
                     keeping the local copy until the update is confirmed"
                );
                SeedEntry {
                    seeded: previous.seeded,
                    update_available: Some(bundled),
                }
            }
            // Identical content: adopt the new baseline. For an absent plugin,
            // retain the receipt so restarts/updates respect its removal.
            SeedAction::Keep => SeedEntry {
                seeded: Some(bundled),
                update_available: None,
            },
        };
        if state.get(&plugin_id) != Some(&next) {
            state.insert(plugin_id, next);
            changed = true;
        }
    }

    if changed {
        write_state(paths, &state);
    }
    Ok(())
}

/// Bundled plugins whose update is waiting for a user decision.
pub fn plugins_with_pending_update(paths: &SmabarPaths) -> BTreeSet<String> {
    read_state(paths)
        .into_iter()
        .filter(|(_, entry)| entry.update_available.is_some())
        .map(|(id, _)| id)
        .collect()
}

/// Removes the data directories and log files of plugins that are gone.
///
/// Returns the ids it cleaned up. Called at startup after seeding and Store
/// recovery, before any concurrent plugin installation can move a folder away.
/// Plugin settings in config.json are retained.
pub fn sweep_orphaned_data(paths: &SmabarPaths) -> Vec<String> {
    sweep_data(paths, None)
}

/// Runtime removal holds this plugin's lifecycle lock; leave other ids alone.
pub(super) fn remove_plugin_data(paths: &SmabarPaths, plugin_id: &str) -> Vec<String> {
    sweep_data(paths, Some(plugin_id))
}

fn sweep_data(paths: &SmabarPaths, only: Option<&str>) -> Vec<String> {
    if crate::store::journal::pending(paths) {
        // ponytail: one journal protects every orphan; resume cleanup after recovery.
        tracing::warn!(
            "store recovery is pending; retaining plugin data and logs until recovery succeeds"
        );
        return Vec::new();
    }
    let mut removed = BTreeSet::new();
    for (plugin_id, path) in orphans(paths) {
        if only.is_some_and(|id| id != plugin_id) {
            continue;
        }
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(()) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    path = %path.display(),
                    "removed a leftover of a plugin that no longer exists"
                );
                removed.insert(plugin_id);
            }
            Err(error) => tracing::warn!(
                plugin_id = %plugin_id,
                path = %path.display(),
                %error,
                "could not remove a plugin leftover"
            ),
        }
    }
    removed.into_iter().collect()
}

/// Every `(plugin id, path)` under `data/` or `logs/` whose plugin folder is
/// gone. Log rotations (`plugin-<id>.log.1`) count as part of their plugin.
fn orphans(paths: &SmabarPaths) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let installed = |plugin_id: &str| paths.plugins_dir().join(plugin_id).is_dir();

    if let Ok(entries) = fs::read_dir(paths.data_dir()) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let plugin_id = entry.file_name().to_string_lossy().into_owned();
            if !installed(&plugin_id) {
                found.push((plugin_id, entry.path()));
            }
        }
    }
    if let Ok(entries) = fs::read_dir(paths.logs_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(rest) = name.strip_prefix("plugin-") else {
                continue;
            };
            let Some(plugin_id) = rest
                .strip_suffix(".log")
                .or_else(|| rest.strip_suffix(".log.1"))
            else {
                continue;
            };
            if !installed(plugin_id) {
                found.push((plugin_id.to_string(), entry.path()));
            }
        }
    }
    found
}

/// Hashes a directory's content: every relative path plus its bytes, in a
/// stable order.
///
/// ponytail: SipHash, not a cryptographic digest — this only has to notice a
/// change, and it costs no dependency. Swap in sha2 if the seed state ever
/// has to survive an untrusted writer.
fn content_hash(dir: &Path) -> io::Result<u64> {
    let mut files = Vec::new();
    collect(dir, PathBuf::new(), &mut files)?;
    files.sort();
    let mut hasher = DefaultHasher::new();
    for relative in files {
        relative.to_string_lossy().hash(&mut hasher);
        fs::read(dir.join(&relative))?.hash(&mut hasher);
    }
    Ok(hasher.finish())
}

fn collect(root: &Path, prefix: PathBuf, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(&prefix))? {
        let entry = entry?;
        if is_build_artifact(&entry.file_name().to_string_lossy()) {
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

/// Entries that are never part of a plugin.
///
/// `__pycache__` is the one that bites: running the plugin (or a linter) in
/// the source tree creates it, it would be copied into the user's folder, and
/// its byte-compiled files change on their own — which would make the content
/// hash report a "new bundled version" that does not exist.
pub(crate) fn is_build_artifact(name: &str) -> bool {
    name == "__pycache__" || name.starts_with('.')
}

/// Copies a bundled plugin into place, cleaning up a half-written folder.
fn install(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    if let Err(error) = copy_contents(source, destination) {
        if let Err(cleanup_error) = fs::remove_dir_all(destination) {
            tracing::warn!(
                %cleanup_error,
                path = %destination.display(),
                "failed to remove a partial bundled plugin seed"
            );
        }
        return Err(error);
    }
    Ok(())
}

fn copy_contents(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if is_build_artifact(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target)?;
            copy_contents(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A missing or broken state file means "no baseline": nothing is overwritten
/// on a guess, so losing it is safe.
fn read_state(paths: &SmabarPaths) -> SeedState {
    let file = paths.seed_state_file();
    let Ok(raw) = fs::read_to_string(&file) else {
        return SeedState::new();
    };
    match serde_json::from_str(&raw) {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(path = %file.display(), %error, "ignoring an unreadable seed state");
            SeedState::new()
        }
    }
}

fn write_state(paths: &SmabarPaths, state: &SeedState) {
    let file = paths.seed_state_file();
    let written = file
        .parent()
        .map(fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| serde_json::to_string_pretty(state).map_err(io::Error::other))
        .and_then(|json| fs::write(&file, json + "\n"));
    if let Err(error) = written {
        // Losing the baseline only costs a confirmation later, never data.
        tracing::warn!(path = %file.display(), %error, "could not persist the seed state");
    }
}
