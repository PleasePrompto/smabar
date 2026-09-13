//! Persistent HTML stays native until its surface needs it. Cache changes and
//! event publication share one lock, so opening content precedes live updates.
//! Each surface receives arrays: one emit carries a whole run of renders.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::Context;
use serde_json::{Value, json};
use smabar_core::plugins::{PluginEvent, UiSnapshot};

use crate::surfaces::{OverlayFlyoutRequest, SurfaceRole};

type UiKey = (String, String, String);

/// Separate event names prevent Tauri 2.11 from evaluating unused payloads in
/// other webviews that registered the same event, even with emit_to filtering.
pub(crate) fn persistent_ui_channel(role: SurfaceRole, target: &str) -> Option<&'static str> {
    match (role, target) {
        (SurfaceRole::Bar, "tile" | "hover") => Some("plugin-ui-bar"),
        (SurfaceRole::Overlay, "hover" | "flyout") => Some("plugin-ui-overlay"),
        _ => None,
    }
}

#[derive(Default)]
struct Render {
    html: String,
    bar_sent: bool,
    overlay_sent: bool,
}

#[derive(Default)]
struct DeliveryState {
    latest: BTreeMap<UiKey, Render>,
    /// Last successful opening delivery, not ownership of the active flyout.
    overlay_generation: Option<u64>,
}

#[derive(Default)]
pub(crate) struct PluginDelivery {
    state: Mutex<DeliveryState>,
}

impl PluginDelivery {
    pub(crate) fn reset(&self, snapshots: Vec<UiSnapshot>) {
        let mut state = self.lock();
        *state = DeliveryState::default();
        for snapshot in snapshots {
            if is_persistent(&snapshot.target) {
                state.latest.insert(
                    (snapshot.plugin_id, snapshot.tile_id, snapshot.target),
                    Render {
                        html: snapshot.html,
                        ..Render::default()
                    },
                );
            }
        }
    }

    pub(crate) fn bar_snapshot(&self) -> Vec<Value> {
        self.lock()
            .latest
            .iter()
            .filter(|(key, _)| persistent_ui_channel(SurfaceRole::Bar, &key.2).is_some())
            .map(|(key, render)| ui_payload(key, &render.html))
            .collect()
    }

    /// Failed delivery or a reattached listener invalidates only delivery
    /// markers. The last one-shot render must remain available for replay.
    pub(crate) fn invalidate(&self) {
        for render in self.lock().latest.values_mut() {
            render.bar_sent = false;
            render.overlay_sent = false;
        }
    }

    /// State changes of the whole run precede its emits: each surface gets
    /// one array with the last HTML per key. Markers are set only after a
    /// successful emit, so a failed bar emit leaves the run pending and skips
    /// the overlay, a failed overlay emit leaves only the overlay pending.
    pub(crate) fn handle_many(
        &self,
        events: &[PluginEvent],
        active: Option<&OverlayFlyoutRequest>,
        mut emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut state = self.lock();
        // ponytail: linear dedupe, a run holds a handful of renders.
        let mut touched: Vec<UiKey> = Vec::new();
        for event in events {
            match event {
                PluginEvent::UiRender {
                    plugin_id,
                    tile_id,
                    target,
                    html,
                    ..
                } if is_persistent(target) => {
                    let key = (plugin_id.clone(), tile_id.clone(), target.clone());
                    let render = state.latest.entry(key.clone()).or_insert_with(|| Render {
                        html: html.clone(),
                        ..Render::default()
                    });
                    if render.html != *html {
                        render.html.clone_from(html);
                        render.bar_sent = false;
                        render.overlay_sent = false;
                    }
                    if !touched.contains(&key) {
                        touched.push(key);
                    }
                }
                PluginEvent::Added {
                    plugin_id, tiles, ..
                } => {
                    state.latest.retain(|(id, tile, _), render| {
                        if id != plugin_id {
                            return true;
                        }
                        render.bar_sent = false;
                        render.overlay_sent = false;
                        tiles.iter().any(|declared| declared.id == *tile)
                    });
                }
                PluginEvent::Removed { plugin_id } => {
                    state.latest.retain(|(id, _, _), _| id != plugin_id);
                }
                _ => {}
            }
        }
        let overlay = active.filter(|request| state.overlay_generation == Some(request.generation));
        state.publish(
            SurfaceRole::Bar,
            touched.iter(),
            None,
            |render| &mut render.bar_sent,
            &mut emit,
        )?;
        if let Some(request) = overlay {
            state.publish(
                SurfaceRole::Overlay,
                touched.iter().filter(|key| matches_flyout(key, request)),
                Some(request.generation),
                |render| &mut render.overlay_sent,
                &mut emit,
            )?;
        }
        Ok(())
    }

    pub(crate) fn open(
        &self,
        request: &OverlayFlyoutRequest,
        with_content: bool,
        mut emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.lock().open(request, with_content, &mut emit)
    }

    pub(crate) fn replay(
        &self,
        active: Option<&OverlayFlyoutRequest>,
        mut emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut state = self.lock();
        for render in state.latest.values_mut() {
            render.bar_sent = false;
        }
        let keys: Vec<UiKey> = state.latest.keys().cloned().collect();
        state.publish(
            SurfaceRole::Bar,
            keys.iter(),
            None,
            |render| &mut render.bar_sent,
            &mut emit,
        )?;
        if let Some(request) = active {
            state.open(request, true, &mut emit)?;
        }
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, DeliveryState> {
        // A panicking holder leaves valid values. Delivery markers are only
        // set after successful emits; the caller still owns surface lifecycle.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DeliveryState {
    /// Emits the unsent renders of `role` among `keys` as one array; the keys
    /// are marked through `sent` only after the emit succeeded.
    fn publish<'k>(
        &mut self,
        role: SurfaceRole,
        keys: impl Iterator<Item = &'k UiKey>,
        generation: Option<u64>,
        sent: fn(&mut Render) -> &mut bool,
        emit: &mut impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut channel = None;
        let mut payload = Vec::new();
        let mut published = Vec::new();
        for key in keys {
            let Some(name) = persistent_ui_channel(role, &key.2) else {
                continue;
            };
            let Some(render) = self.latest.get_mut(key) else {
                continue;
            };
            if *sent(render) {
                continue;
            }
            let mut item = ui_payload(key, &render.html);
            if let Some(generation) = generation {
                item["generation"] = json!(generation);
            }
            payload.push(item);
            published.push(key);
            channel = Some(name);
        }
        let Some(channel) = channel else {
            return Ok(());
        };
        emit(channel, Value::Array(payload))
            .with_context(|| format!("failed to deliver plugin HTML on {channel}"))?;
        for key in published {
            if let Some(render) = self.latest.get_mut(key) {
                *sent(render) = true;
            }
        }
        Ok(())
    }

    fn open(
        &mut self,
        request: &OverlayFlyoutRequest,
        with_content: bool,
        emit: &mut impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut payload = serde_json::to_value(request)?;
        if with_content {
            let mut hover = None;
            let mut flyout = None;
            for (key, render) in &mut self.latest {
                render.overlay_sent = false;
                if matches_flyout(key, request) {
                    match key.2.as_str() {
                        "hover" => hover = Some(render.html.as_str()),
                        "flyout" => flyout = Some(render.html.as_str()),
                        _ => {}
                    }
                }
            }
            // Explicit nulls clear a previous generation's absent target.
            payload["content"] = json!({ "hover": hover, "flyout": flyout });
        }
        emit("surface-flyout", payload).context("failed to deliver the flyout request")?;
        if with_content {
            self.overlay_generation = Some(request.generation);
            for (key, render) in &mut self.latest {
                render.overlay_sent = matches_flyout(key, request)
                    && persistent_ui_channel(SurfaceRole::Overlay, &key.2).is_some();
            }
        }
        Ok(())
    }
}

fn is_persistent(target: &str) -> bool {
    matches!(target, "tile" | "hover" | "flyout")
}

fn matches_flyout(key: &UiKey, request: &OverlayFlyoutRequest) -> bool {
    request
        .tile_id
        .strip_prefix("plugin:")
        .and_then(|id| id.split_once(':'))
        == Some((key.0.as_str(), key.1.as_str()))
}

/// Persistent renders carry no popup lifetime; the shell reads `ttlMs` only
/// for popups.
fn ui_payload(key: &UiKey, html: &str) -> Value {
    json!({
        "pluginId": key.0,
        "tileId": key.1,
        "target": key.2,
        "html": html,
    })
}

#[cfg(test)]
#[path = "plugin_delivery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "plugin_delivery_batch_tests.rs"]
mod batch_tests;
