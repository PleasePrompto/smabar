//! Queryable plugin status: mirrors the supervisor's own event stream into a
//! map and merges it with the registered plugins for introspection (MCP).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use super::activation;
use super::manifest::PluginManifest;
use super::supervisor::PluginSupervisor;
use crate::util::lock_unpoisoned;

use super::{PluginEvent, PluginStatus};

/// Latest tracked lifecycle state of one plugin id (or plugin folder name,
/// for folders whose manifest never loaded).
#[derive(Debug, Clone)]
pub(super) struct StatusEntry {
    pub(super) status: PluginStatus,
    pub(super) error: Option<String>,
}

pub(super) type StatusMap = Arc<Mutex<HashMap<String, StatusEntry>>>;

/// One cached tile render, from [`super::PluginSupervisor::current_ui`].
#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub plugin_id: String,
    pub tile_id: String,
    pub target: String,
    pub html: String,
}

/// Introspection snapshot of one plugin, from
/// [`super::PluginSupervisor::plugin_infos`].
///
/// `dir` and `manifest` are `None` for status-only entries: folders whose
/// manifest failed validation, keyed by folder name.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub dir: Option<PathBuf>,
    pub manifest: Option<PluginManifest>,
    pub status: PluginStatus,
    pub error: Option<String>,
}

impl PluginSupervisor {
    /// Manifests of all currently registered plugins. Late subscribers (the
    /// shell attaches after startup) use this instead of the missed `Added`
    /// events.
    pub fn current_plugins(&self) -> Vec<PluginManifest> {
        lock_unpoisoned(&self.inner.plugins)
            .values()
            .map(|handle| handle.manifest.clone())
            .collect()
    }

    /// Introspection snapshot of every known plugin, sorted by id: registered
    /// plugins, deactivated ones, and status-only broken folders.
    pub fn plugin_infos(&self) -> Vec<PluginInfo> {
        let registered: Vec<(String, PathBuf, PluginManifest)> =
            lock_unpoisoned(&self.inner.plugins)
                .iter()
                .map(|(id, handle)| (id.clone(), handle.dir.clone(), handle.manifest.clone()))
                .collect();
        let deactivated = activation::entries(&self.inner);
        let statuses = lock_unpoisoned(&self.inner.statuses);
        merge_infos(registered, deactivated, &statuses)
    }

    /// The installed folder for a plugin id, if one exists.
    pub fn plugin_dir(&self, plugin_id: &str) -> Option<PathBuf> {
        let known = self
            .plugin_infos()
            .into_iter()
            .find(|info| info.id == plugin_id)
            .and_then(|info| info.dir);
        if known.is_some() {
            return known;
        }
        let dir = self.inner.paths.plugins_dir().join(plugin_id);
        dir.is_dir().then_some(dir)
    }

    /// Last rendered HTML of every tile target, sorted for determinism.
    pub fn current_ui(&self) -> Vec<UiSnapshot> {
        self.inner.events.current_ui()
    }

    /// Atomically captures persistent UI and subscribes to subsequent events.
    /// A render concurrent with this call is either reflected in the snapshot
    /// or received afterward, never lost between the two. Popups are not replayed.
    pub fn subscribe_with_ui(&self) -> (Vec<UiSnapshot>, broadcast::Receiver<PluginEvent>) {
        self.inner.events.subscribe_with_ui()
    }

    /// Drains the receiver's retained events and captures persistent UI at the
    /// same publication boundary. Forward the returned lifecycle/popup events
    /// in order, apply the authoritative snapshot, then continue this receiver.
    /// Events already overwritten by the broadcast channel cannot be recovered.
    pub fn resync_ui(
        &self,
        receiver: &mut broadcast::Receiver<PluginEvent>,
    ) -> (Vec<UiSnapshot>, Vec<PluginEvent>) {
        self.inner.events.resync_ui(receiver)
    }

    /// Tile ids of `plugin_id` whose tile rendered at or after `since_ms`,
    /// sorted. The reload reply uses it to say which declared tiles are
    /// still blank after a start.
    pub fn tiles_rendered_since(&self, plugin_id: &str, since_ms: u64) -> Vec<String> {
        self.inner.events.tiles_rendered_since(plugin_id, since_ms)
    }
}

/// Upserts `Status` events and drops a plugin's status on `Removed`.
/// UI is already cached synchronously before publication. Ends when the
/// supervisor (the event sender) is dropped.
pub(super) async fn track_events(mut rx: broadcast::Receiver<PluginEvent>, statuses: StatusMap) {
    loop {
        match rx.recv().await {
            Ok(PluginEvent::Status {
                plugin_id,
                status,
                error,
            }) => {
                lock_unpoisoned(&statuses).insert(plugin_id, StatusEntry { status, error });
            }
            Ok(PluginEvent::Removed { plugin_id }) => {
                lock_unpoisoned(&statuses).remove(&plugin_id);
            }
            Ok(PluginEvent::UiRender { .. } | PluginEvent::Added { .. }) => {}
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "plugin status tracker lagged; some updates were missed"
                );
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Merges registered plugins (default status `Starting` until the first
/// status event), deactivated plugins, and status-only entries, sorted by id.
///
/// A deactivated plugin's status is not read from the status map but fixed to
/// [`PluginStatus::Deactivated`]: it is a config fact, not a lifecycle
/// observation, and the map may still hold the `stopped` from the shutdown
/// that switching it off caused.
pub(super) fn merge_infos(
    registered: Vec<(String, PathBuf, PluginManifest)>,
    deactivated: Vec<(String, PathBuf, PluginManifest)>,
    statuses: &HashMap<String, StatusEntry>,
) -> Vec<PluginInfo> {
    let mut infos: Vec<PluginInfo> = registered
        .into_iter()
        .map(|(id, dir, manifest)| {
            let (status, error) = statuses
                .get(&id)
                .map(|entry| (entry.status, entry.error.clone()))
                .unwrap_or((PluginStatus::Starting, None));
            PluginInfo {
                id,
                dir: Some(dir),
                manifest: Some(manifest),
                status,
                error,
            }
        })
        .collect();
    for (id, dir, manifest) in deactivated {
        if infos.iter().any(|info| info.id == id) {
            continue;
        }
        infos.push(PluginInfo {
            id,
            dir: Some(dir),
            manifest: Some(manifest),
            status: PluginStatus::Deactivated,
            error: None,
        });
    }
    for (id, entry) in statuses {
        if infos.iter().any(|info| info.id == *id) {
            continue;
        }
        infos.push(PluginInfo {
            id: id.clone(),
            dir: None,
            manifest: None,
            status: entry.status,
            error: entry.error.clone(),
        });
    }
    infos.sort_by(|a, b| a.id.cmp(&b.id));
    infos
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str) -> PluginManifest {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "version": "1.0.0",
            "protocolVersion": 1,
            "runtime": "exec",
            "command": ["true"],
            "tiles": [{ "id": "w", "name": "W" }],
        }))
        .expect("valid manifest")
    }

    #[test]
    fn merge_unions_registered_and_status_only_entries_sorted() {
        let registered = vec![
            (
                "zeta".to_string(),
                PathBuf::from("/p/zeta"),
                manifest("zeta"),
            ),
            (
                "alpha".to_string(),
                PathBuf::from("/p/alpha"),
                manifest("alpha"),
            ),
        ];
        let statuses = HashMap::from([
            (
                "zeta".to_string(),
                StatusEntry {
                    status: PluginStatus::Running,
                    error: None,
                },
            ),
            (
                "broken-folder".to_string(),
                StatusEntry {
                    status: PluginStatus::Failed,
                    error: Some("bad manifest".to_string()),
                },
            ),
        ]);

        let infos = merge_infos(registered, Vec::new(), &statuses);
        let ids: Vec<&str> = infos.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "broken-folder", "zeta"]);

        // Registered without a status event yet defaults to Starting.
        assert_eq!(infos[0].status, PluginStatus::Starting);
        assert!(infos[0].manifest.is_some());
        assert_eq!(
            infos[0].dir.as_deref(),
            Some(std::path::Path::new("/p/alpha"))
        );

        // Status-only entry carries the failure but no manifest/dir.
        assert_eq!(infos[1].status, PluginStatus::Failed);
        assert_eq!(infos[1].error.as_deref(), Some("bad manifest"));
        assert!(infos[1].manifest.is_none());
        assert!(infos[1].dir.is_none());

        assert_eq!(infos[2].status, PluginStatus::Running);
    }

    /// A deactivated plugin must appear as `deactivated`, not as the
    /// `stopped` its own shutdown left in the status map — otherwise the
    /// settings panel cannot tell "the user switched this off" from "this
    /// died".
    #[test]
    fn deactivated_plugins_are_listed_with_a_status_of_their_own() {
        let registered = vec![(
            "running".to_string(),
            PathBuf::from("/p/running"),
            manifest("running"),
        )];
        let deactivated = vec![("off".to_string(), PathBuf::from("/p/off"), manifest("off"))];
        let statuses = HashMap::from([(
            "off".to_string(),
            StatusEntry {
                status: PluginStatus::Stopped,
                error: None,
            },
        )]);

        let infos = merge_infos(registered, deactivated, &statuses);
        let ids: Vec<&str> = infos.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, vec!["off", "running"]);
        assert_eq!(infos[0].status, PluginStatus::Deactivated);
        assert!(
            infos[0].manifest.is_some(),
            "its manifest is kept so the UI can show its name and tiles"
        );
        assert_eq!(
            infos[0].dir.as_deref(),
            Some(std::path::Path::new("/p/off"))
        );
        assert_eq!(infos[1].status, PluginStatus::Starting);
    }
}
