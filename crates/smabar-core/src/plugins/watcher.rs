//! Hot reload: directory/config watches and their lifecycle reactions.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use crate::config::SmabarConfig;

use super::PluginEvent;
use super::activation;
use super::manifest::{MANIFEST_FILE, PluginManifest};
use super::runner::PluginCommand;
use super::supervisor::{
    Inner, lifecycle_guard, manifest_start_error, start_loaded_plugin, start_plugin, stop_handle,
};
use crate::util::lock_unpoisoned;

/// Quiet period after the last folder event before changes are applied.
pub(super) const DIR_DEBOUNCE: Duration = Duration::from_millis(500);

/// Sorted subdirectories of the plugins dir that contain a manifest file.
pub(super) fn list_plugin_dirs(plugins_dir: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, path = %plugins_dir.display(), "cannot scan plugins directory");
            return Vec::new();
        }
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(MANIFEST_FILE).is_file())
        .collect();
    dirs.sort();
    dirs
}

/// Starts the recursive watch on the plugins directory. Returns `None` (with
/// a warning) when the OS watch cannot be created.
pub(super) fn spawn_dir_watcher(
    plugins_dir: &Path,
    tx: mpsc::UnboundedSender<String>,
) -> Option<RecommendedWatcher> {
    let root = plugins_dir.to_path_buf();
    let handler = move |result: Result<notify::Event, notify::Error>| match result {
        Ok(event) => {
            for path in &event.paths {
                if let Some(folder) = folder_for_event(&root, &event.kind, path) {
                    // Send fails only while the supervisor is being dropped.
                    let _ = tx.send(folder);
                }
            }
        }
        Err(error) => tracing::warn!(%error, "plugins directory watcher error"),
    };
    let mut watcher = match notify::recommended_watcher(handler) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::warn!(%error, "cannot create plugins directory watcher; hot reload disabled");
            return None;
        }
    };
    if let Err(error) = watcher.watch(plugins_dir, RecursiveMode::Recursive) {
        tracing::warn!(
            %error,
            path = %plugins_dir.display(),
            "cannot watch plugins directory; hot reload disabled"
        );
        return None;
    }
    Some(watcher)
}

/// The plugin folder an event should reload, if any. Reads never count. A
/// content or metadata change reported on a DIRECTORY is dropped too:
/// Windows reports the parent folder as modified whenever an entry appears
/// inside it, so the first `__pycache__` of a fresh profile would otherwise
/// restart every plugin right after its first start. The entry itself
/// arrives as its own event and is judged by [`affected_folder`]; folder
/// renames stay `Modify(Name)` and are kept, as are create and remove.
fn folder_for_event(root: &Path, kind: &EventKind, changed: &Path) -> Option<String> {
    if kind.is_access() {
        return None;
    }
    if matches!(
        kind,
        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_))
    ) && changed.is_dir()
    {
        return None;
    }
    affected_folder(root, changed)
}

/// Maps a changed path to the top-level plugin folder it belongs to.
/// Dotfile and `__pycache__` churn is ignored so a plugin writing cache
/// files into its own data dir does not restart itself over them.
fn affected_folder(root: &Path, changed: &Path) -> Option<String> {
    let rel = changed.strip_prefix(root).ok()?;
    let ignored = rel.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "__pycache__" || name.starts_with('.')
    });
    if ignored {
        return None;
    }
    let first = rel.components().next()?;
    Some(first.as_os_str().to_string_lossy().into_owned())
}

/// Debounces watcher events (quiet period per burst) and applies the
/// resulting folder changes one by one.
pub(super) async fn dir_change_loop(inner: Arc<Inner>, mut rx: mpsc::UnboundedReceiver<String>) {
    loop {
        let first = tokio::select! {
            biased;
            () = inner.shutdown.cancelled() => return,
            first = rx.recv() => match first {
                Some(first) => first,
                None => return,
            },
        };
        let mut affected = BTreeSet::from([first]);
        loop {
            let changed = tokio::select! {
                biased;
                () = inner.shutdown.cancelled() => return,
                changed = tokio::time::timeout(DIR_DEBOUNCE, rx.recv()) => changed,
            };
            match changed {
                Ok(Some(folder)) => {
                    affected.insert(folder);
                }
                Ok(None) => return,
                Err(_) => break,
            }
        }
        for folder in affected {
            apply_folder_change(&inner, &folder).await;
        }
    }
}

/// The supervisor's whole reaction to config changes: per-plugin settings
/// pushes, and starting or stopping processes when `pluginsDeactivated`
/// changes. One subscription, so the two can never observe different configs.
pub(super) async fn config_change_loop(
    inner: Arc<Inner>,
    mut rx: broadcast::Receiver<crate::config::ConfigChange>,
    mut applied: SmabarConfig,
) {
    loop {
        let received = tokio::select! {
            biased;
            () = inner.shutdown.cancelled() => return,
            received = rx.recv() => received,
        };
        let next = match received {
            Ok(change) => change.new,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "config change stream lagged; reconciling plugin state with the latest config"
                );
                // The retained intermediate snapshots are now stale. Drain
                // them, then converge once on ConfigWatcher's authoritative
                // current value. A concurrent newer event remains queued and
                // is harmless if the snapshot already includes it.
                loop {
                    match rx.try_recv() {
                        Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                        Err(broadcast::error::TryRecvError::Empty) => break,
                        Err(broadcast::error::TryRecvError::Closed) => return,
                    }
                }
                inner.config.current()
            }
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if inner.shutdown.is_cancelled() {
            return;
        }
        apply_config_change(&inner, &applied, &next).await;
        applied = next;
    }
}

async fn apply_config_change(inner: &Arc<Inner>, old: &SmabarConfig, new: &SmabarConfig) {
    if old.plugins_deactivated != new.plugins_deactivated {
        activation::reconcile(inner, &old.plugins_deactivated, &new.plugins_deactivated).await;
    }
    let notifications: Vec<_> = lock_unpoisoned(&inner.plugins)
        .iter()
        .filter(|(id, _)| old.plugins.get(*id) != new.plugins.get(*id))
        .map(|(id, handle)| {
            let settings = new.plugins.get(id).cloned().unwrap_or_else(|| json!({}));
            (id.clone(), handle.commands.clone(), settings)
        })
        .collect();
    for (id, commands, settings) in notifications {
        tracing::debug!(plugin = %id, "settings changed on disk; notifying plugin");
        // Err = lifecycle ended; the change is persisted regardless.
        let _ = commands
            .send(PluginCommand::SettingsChanged { settings })
            .await;
    }
}

/// Restarts, starts, or removes the plugin backed by `folder`.
///
/// A DEACTIVATED plugin is never started from here: the change still runs
/// through [`start_plugin`], which files the new manifest and stops there, so
/// editing a switched-off plugin's code does not bring its process back.
async fn apply_folder_change(inner: &Arc<Inner>, folder: &str) {
    let dir = inner.paths.plugins_dir().join(folder);
    let existing_id = lock_unpoisoned(&inner.plugins)
        .iter()
        .find_map(|(id, handle)| (handle.dir == dir).then(|| id.clone()));
    let plugin_id = existing_id
        .or_else(|| PluginManifest::load(&dir).ok().map(|manifest| manifest.id))
        .unwrap_or_else(|| folder.to_string());
    let Some((_transition, _lifecycle)) = lifecycle_guard(inner, &plugin_id).await else {
        return;
    };

    // Re-read after acquiring the lifecycle lock: an explicit restart may
    // have replaced the handle while this watcher event was queued.
    let existing_id = {
        let plugins = lock_unpoisoned(&inner.plugins);
        plugins
            .iter()
            .find_map(|(id, handle)| (handle.dir == dir).then(|| id.clone()))
    };
    let has_manifest = dir.join(MANIFEST_FILE).is_file();
    if let Some(id) = existing_id {
        let candidate = if has_manifest {
            match PluginManifest::load(&dir) {
                Ok(manifest) => {
                    if let Some(error) = manifest_start_error(&dir, &manifest) {
                        warn_bad_reload(inner, &id, &error);
                        return;
                    }
                    Some(manifest)
                }
                Err(error) => {
                    warn_bad_reload(inner, &id, &error.to_string());
                    return;
                }
            }
        } else {
            None
        };
        let Some(handle) = lock_unpoisoned(&inner.plugins).remove(&id) else {
            return;
        };
        tracing::info!(plugin = %id, "plugin folder changed; stopping the current instance");
        stop_handle(handle).await;
        if let Some(manifest) = candidate {
            start_loaded_plugin(inner, &dir, manifest);
        } else {
            let _ = inner.events.send(PluginEvent::Removed { plugin_id: id });
        }
        return;
    } else if !has_manifest {
        // A deactivated plugin has no handle but is still installed; when its
        // folder goes away it has to stop being listed as installed.
        if let Some(id) = activation::deactivated_id_for_dir(inner, &dir) {
            activation::forget(inner, &id);
            tracing::info!(plugin = %id, "deactivated plugin's folder disappeared; forgetting it");
            let _ = inner.events.send(PluginEvent::Removed { plugin_id: id });
            return;
        }
        // Not a plugin folder and nothing was running from it — e.g. a
        // half-copied folder whose manifest arrives with a later event.
        tracing::debug!(folder, "ignoring change in folder without a manifest");
        return;
    }
    start_plugin(inner, &dir);
}

fn warn_bad_reload(inner: &Inner, plugin_id: &str, reason: &str) {
    let message = format!(
        "smabar.json reload was ignored because {reason}; the current plugin was kept. Fix the \
         manifest using plugin_guide, then save it again."
    );
    inner.diagnostics.warn_once(plugin_id, &message, None);
}

#[cfg(test)]
mod tests {
    use notify::event::{DataChange, RenameMode};

    use super::*;

    #[test]
    fn folder_modifications_are_ignored_but_their_entries_and_renames_are_not() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("clock/__pycache__")).expect("plugin dir");
        fs::write(root.join("clock/plugin.py"), b"x").expect("script");
        let clock = root.join("clock");
        let modified = [
            EventKind::Modify(ModifyKind::Any),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
        ];
        for kind in &modified {
            assert_eq!(
                folder_for_event(root, kind, &clock),
                None,
                "{kind:?} on the folder"
            );
            assert_eq!(
                folder_for_event(root, kind, &clock.join("plugin.py")),
                Some("clock".into()),
                "{kind:?} on a file"
            );
        }
        assert_eq!(
            folder_for_event(
                root,
                &EventKind::Create(notify::event::CreateKind::Any),
                &clock.join("__pycache__")
            ),
            None
        );
        for kind in [
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Create(notify::event::CreateKind::Folder),
            EventKind::Remove(notify::event::RemoveKind::Folder),
        ] {
            assert_eq!(
                folder_for_event(root, &kind, &clock),
                Some("clock".into()),
                "{kind:?}"
            );
        }
        assert_eq!(
            folder_for_event(
                root,
                &EventKind::Access(notify::event::AccessKind::Any),
                &clock.join("plugin.py")
            ),
            None
        );
    }
}
