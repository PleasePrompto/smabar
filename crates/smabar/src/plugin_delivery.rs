//! Persistent HTML stays native until its surface needs it. Cache changes and
//! event publication share one lock, so opening content precedes live updates.

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
            .map(|(key, render)| ui_payload(key, &render.html, None))
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

    pub(crate) fn handle(
        &self,
        event: &PluginEvent,
        active: Option<&OverlayFlyoutRequest>,
        mut emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut state = self.lock();
        match event {
            PluginEvent::UiRender {
                plugin_id,
                tile_id,
                target,
                html,
                ttl_ms,
            } if is_persistent(target) => {
                let key = (plugin_id.clone(), tile_id.clone(), target.clone());
                let overlay_request = active.filter(|request| {
                    state.overlay_generation == Some(request.generation)
                        && matches_flyout(&key, request)
                });
                let render = state.latest.entry(key.clone()).or_insert_with(|| Render {
                    html: html.clone(),
                    ..Render::default()
                });
                if render.html != *html {
                    render.html.clone_from(html);
                    render.bar_sent = false;
                    render.overlay_sent = false;
                }
                if !render.bar_sent
                    && let Some(channel) = persistent_ui_channel(SurfaceRole::Bar, target)
                {
                    emit(channel, ui_payload(&key, &render.html, *ttl_ms))
                        .context("failed to deliver plugin HTML to the bar")?;
                    render.bar_sent = true;
                }
                if !render.overlay_sent
                    && let Some(request) = overlay_request
                    && let Some(channel) = persistent_ui_channel(SurfaceRole::Overlay, target)
                {
                    let mut payload = ui_payload(&key, &render.html, *ttl_ms);
                    payload["generation"] = json!(request.generation);
                    emit(channel, payload)
                        .context("failed to deliver plugin HTML to the open flyout")?;
                    render.overlay_sent = true;
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
        for (key, render) in &mut state.latest {
            if let Some(channel) = persistent_ui_channel(SurfaceRole::Bar, &key.2) {
                emit(channel, ui_payload(key, &render.html, None))
                    .context("failed to replay plugin HTML to the bar")?;
                render.bar_sent = true;
            }
        }
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

fn ui_payload(key: &UiKey, html: &str, ttl_ms: Option<u32>) -> Value {
    let mut payload = json!({
        "pluginId": key.0,
        "tileId": key.1,
        "target": key.2,
        "html": html,
    });
    if let Some(ttl_ms) = ttl_ms {
        payload["ttlMs"] = json!(ttl_ms);
    }
    payload
}

#[cfg(test)]
#[path = "plugin_delivery_tests.rs"]
mod tests;
