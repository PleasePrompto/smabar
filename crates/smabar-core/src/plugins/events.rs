//! The plugin event stream and its persistent UI snapshot share one boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use super::{PluginEvent, UiSnapshot};
use crate::util::{lock_unpoisoned, now_ms};

/// Last persistent render per (plugin, tile, target), with its render time.
type UiMap = HashMap<(String, String, String), (String, u64)>;

#[derive(Clone)]
pub(crate) struct PluginEvents {
    sender: broadcast::Sender<PluginEvent>,
    ui: Arc<Mutex<UiMap>>,
}

impl PluginEvents {
    pub(super) fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            ui: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(super) fn send(
        &self,
        event: PluginEvent,
    ) -> Result<usize, broadcast::error::SendError<()>> {
        let mut ui = lock_unpoisoned(&self.ui);
        record_ui(&mut ui, &event, now_ms());
        // Hold the cache lock through publication: a snapshot plus receiver
        // must place every render on exactly one side of its boundary.
        // Callers only need to know whether listeners exist; returning the
        // entire already-cached event would make the error unnecessarily large.
        self.sender
            .send(event)
            .map_err(|_| broadcast::error::SendError(()))
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<PluginEvent> {
        self.sender.subscribe()
    }

    pub(super) fn subscribe_with_ui(&self) -> (Vec<UiSnapshot>, broadcast::Receiver<PluginEvent>) {
        let ui = lock_unpoisoned(&self.ui);
        let receiver = self.sender.subscribe();
        (snapshots(&ui), receiver)
    }

    pub(super) fn resync_ui(
        &self,
        receiver: &mut broadcast::Receiver<PluginEvent>,
    ) -> (Vec<UiSnapshot>, Vec<PluginEvent>) {
        let ui = lock_unpoisoned(&self.ui);
        let mut pending = Vec::new();
        let mut skipped = 0_u64;
        // Publication holds this same lock. Draining the existing receiver
        // retains its lifecycle/popup tail and leaves it exactly at the
        // snapshot boundary, including when it has already reported Lagged.
        loop {
            match receiver.try_recv() {
                Ok(event) => pending.push(event),
                Err(broadcast::error::TryRecvError::Lagged(count)) => {
                    skipped = skipped.saturating_add(count);
                }
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => {
                    break;
                }
            }
        }
        let snapshot = snapshots(&ui);
        drop(ui);
        if skipped > 0 {
            tracing::warn!(
                skipped,
                "plugin events were lost before UI resynchronization"
            );
        }
        (snapshot, pending)
    }

    pub(super) fn current_ui(&self) -> Vec<UiSnapshot> {
        snapshots(&lock_unpoisoned(&self.ui))
    }

    pub(super) fn tiles_rendered_since(&self, plugin_id: &str, since_ms: u64) -> Vec<String> {
        let mut tiles: Vec<String> = lock_unpoisoned(&self.ui)
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

fn record_ui(ui: &mut UiMap, event: &PluginEvent, rendered_at: u64) {
    match event {
        PluginEvent::UiRender {
            plugin_id,
            tile_id,
            target,
            html,
            ..
        } if target != "popup" => {
            ui.insert(
                (plugin_id.clone(), tile_id.clone(), target.clone()),
                (html.clone(), rendered_at),
            );
        }
        PluginEvent::Added {
            plugin_id, tiles, ..
        } => {
            // A restart can drop tiles while retaining the last HTML of the
            // surviving ones until the plugin renders them again.
            ui.retain(|(id, tile, _), _| {
                id != plugin_id || tiles.iter().any(|declared| declared.id == *tile)
            });
        }
        PluginEvent::Removed { plugin_id } => {
            ui.retain(|(id, _, _), _| id != plugin_id);
        }
        _ => {}
    }
}

fn snapshots(ui: &UiMap) -> Vec<UiSnapshot> {
    let mut snapshots: Vec<UiSnapshot> = ui
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

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
