//! Queryable plugin status: mirrors the supervisor's own event stream into a
//! map and merges it with the registered plugins for introspection (MCP).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use super::activation;
use super::manifest::PluginManifest;
use super::supervisor::PluginSupervisor;
use crate::util::{lock_unpoisoned, now_ms};

#[cfg(test)]
use super::PluginTileDef;
use super::{PluginEvent, PluginStatus};

/// Latest tracked lifecycle state of one plugin id (or plugin folder name,
/// for folders whose manifest never loaded).
#[derive(Debug, Clone)]
pub(super) struct StatusEntry {
    pub(super) status: PluginStatus,
    pub(super) error: Option<String>,
}

pub(super) type StatusMap = Arc<Mutex<HashMap<String, StatusEntry>>>;

/// Last persistent render per (plugin id, tile id, target), with the
/// millisecond timestamp of that render. Late subscribers fetch
/// tile/flyout/hover state instead of waiting for the plugin's next push, and
/// a reload reply can tell which tiles rendered in THIS start. Transient
/// popups are deliberately never replayed.
pub(super) type UiMap = Arc<Mutex<HashMap<(String, String, String), (String, u64)>>>;

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
        let mut snapshots: Vec<UiSnapshot> = lock_unpoisoned(&self.inner.ui)
            .iter()
            .map(|((plugin_id, tile_id, target), (html, _))| UiSnapshot {
                plugin_id: plugin_id.clone(),
                tile_id: tile_id.clone(),
                target: target.clone(),
                html: html.clone(),
            })
            .collect();
        snapshots.sort_by(|a, b| {
            (&a.plugin_id, &a.tile_id, &a.target).cmp(&(&b.plugin_id, &b.tile_id, &b.target))
        });
        snapshots
    }

    /// Tile ids of `plugin_id` whose tile rendered at or after `since_ms`,
    /// sorted. The reload reply uses it to say which declared tiles are
    /// still blank after a start.
    pub fn tiles_rendered_since(&self, plugin_id: &str, since_ms: u64) -> Vec<String> {
        let mut tiles: Vec<String> = lock_unpoisoned(&self.inner.ui)
            .iter()
            .filter(|((id, _, target), (_, at))| {
                id == plugin_id && target == "tile" && *at >= since_ms
            })
            .map(|((_, tile, _), _)| tile.clone())
            .collect();
        tiles.sort();
        tiles
    }
}

/// Upserts `Status` events into the status map, caches `UiRender` HTML, and
/// drops a plugin's entries on `Removed`. Ends when the supervisor (the
/// event sender) is dropped.
pub(super) async fn track_events(
    mut rx: broadcast::Receiver<PluginEvent>,
    statuses: StatusMap,
    ui: UiMap,
) {
    loop {
        match rx.recv().await {
            Ok(PluginEvent::Status {
                plugin_id,
                status,
                error,
            }) => {
                lock_unpoisoned(&statuses).insert(plugin_id, StatusEntry { status, error });
            }
            Ok(PluginEvent::UiRender {
                plugin_id,
                tile_id,
                target,
                html,
                ttl_ms: _,
            }) => {
                if target != "popup" {
                    lock_unpoisoned(&ui).insert((plugin_id, tile_id, target), (html, now_ms()));
                }
            }
            Ok(PluginEvent::Added {
                plugin_id, tiles, ..
            }) => {
                // A restart re-emits Added with the CURRENT manifest, so this
                // is where a manifest that dropped a tile is noticed. Without
                // the purge, current_ui() keeps serving the dead tile's HTML
                // to every webview that attaches later.
                lock_unpoisoned(&ui).retain(|(id, tile, _), _| {
                    *id != plugin_id || tiles.iter().any(|declared| declared.id == *tile)
                });
            }
            Ok(PluginEvent::Removed { plugin_id }) => {
                lock_unpoisoned(&statuses).remove(&plugin_id);
                lock_unpoisoned(&ui).retain(|(id, _, _), _| *id != plugin_id);
            }
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

    #[tokio::test]
    async fn a_restart_forgets_the_ui_of_tiles_the_manifest_dropped() {
        let (tx, rx) = broadcast::channel(16);
        let statuses: StatusMap = Arc::new(Mutex::new(HashMap::new()));
        let ui: UiMap = Arc::new(Mutex::new(HashMap::new()));
        let tracker = tokio::spawn(track_events(rx, Arc::clone(&statuses), Arc::clone(&ui)));

        let added = |tiles: Vec<&str>| PluginEvent::Added {
            plugin_id: "demo".to_string(),
            name: "Demo".to_string(),
            icon_data_url: None,
            tiles: tiles
                .into_iter()
                .map(|id| PluginTileDef {
                    id: id.to_string(),
                    name: id.to_string(),
                    has_flyout: false,
                    tile: None,
                    tile_scale: None,
                    icon_svg: None,
                    use_plugin_icon: false,
                    accent: None,
                    accent_2: None,
                    accent_fg: None,
                })
                .collect(),
            settings_schema: None,
        };
        let render = |tile: &str| PluginEvent::UiRender {
            plugin_id: "demo".to_string(),
            tile_id: tile.to_string(),
            target: "tile".to_string(),
            html: "<b>x</b>".to_string(),
            ttl_ms: None,
        };
        tx.send(added(vec!["one", "two"])).expect("send added");
        tx.send(render("one")).expect("send render");
        tx.send(render("two")).expect("send render");
        // The manifest now declares only "one" — "two" must not survive, or
        // current_ui() would keep serving a tile that no longer exists.
        tx.send(added(vec!["one"])).expect("send shrunken added");
        drop(tx);
        tracker.await.expect("tracker ends with the sender");

        let cached: Vec<String> = lock_unpoisoned(&ui)
            .keys()
            .map(|(_, tile, _)| tile.clone())
            .collect();
        assert_eq!(cached, vec!["one".to_string()]);
    }
}
