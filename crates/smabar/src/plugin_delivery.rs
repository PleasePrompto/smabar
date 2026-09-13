//! Persistent HTML stays native until its surface needs it. Cache changes and
//! event publication share one lock, so opening content precedes live updates.
//! Live updates only signal a surface, which pulls the pending HTML through a
//! command: Tauri evaluates event payloads as script source that WebKit keeps,
//! while command responses travel as IPC bytes.

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
    /// The bar pulled this HTML or received it in its startup snapshot.
    bar_sent: bool,
    /// The open flyout pulled this HTML or received it while opening.
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

    /// Every bar render for the startup snapshot; it counts as delivered.
    pub(crate) fn bar_snapshot(&self) -> Vec<Value> {
        self.lock()
            .latest
            .iter_mut()
            .filter(|(key, _)| persistent_ui_channel(SurfaceRole::Bar, &key.2).is_some())
            .map(|(key, render)| {
                render.bar_sent = true;
                ui_payload(key, &render.html)
            })
            .collect()
    }

    /// Pending HTML for `role`; it counts as delivered once returned, so a
    /// failed pull is repeated after the next signal.
    pub(crate) fn take(
        &self,
        role: SurfaceRole,
        active: Option<&OverlayFlyoutRequest>,
    ) -> Vec<Value> {
        let mut state = self.lock();
        let opened = state.overlay_generation;
        let request = active.filter(|request| opened == Some(request.generation));
        state
            .latest
            .iter_mut()
            .filter_map(|(key, render)| {
                persistent_ui_channel(role, &key.2)?;
                match role {
                    SurfaceRole::Bar if !render.bar_sent => {
                        render.bar_sent = true;
                        Some(ui_payload(key, &render.html))
                    }
                    SurfaceRole::Overlay if !render.overlay_sent => {
                        let request = request.filter(|request| matches_flyout(key, request))?;
                        render.overlay_sent = true;
                        let mut payload = ui_payload(key, &render.html);
                        payload["generation"] = json!(request.generation);
                        Some(payload)
                    }
                    _ => None,
                }
            })
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

    /// State changes of the whole run precede one signal per surface with
    /// pending HTML; the surface then pulls it with `take`.
    pub(crate) fn handle_many(
        &self,
        events: &[PluginEvent],
        active: Option<&OverlayFlyoutRequest>,
        mut emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut state = self.lock();
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
                    let render = state.latest.entry(key).or_insert_with(|| Render {
                        html: html.clone(),
                        ..Render::default()
                    });
                    if render.html != *html {
                        render.html.clone_from(html);
                        render.bar_sent = false;
                        render.overlay_sent = false;
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
        state.signal(active, &mut emit)
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
        state.signal(None, &mut emit)?;
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
    /// One tiny signal per surface with pending HTML. A failed signal keeps
    /// the HTML pending; the next run signals again.
    fn signal(
        &self,
        active: Option<&OverlayFlyoutRequest>,
        emit: &mut impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if self.latest.iter().any(|(key, render)| {
            !render.bar_sent && persistent_ui_channel(SurfaceRole::Bar, &key.2).is_some()
        }) {
            emit("plugin-ui-bar", json!({}))
                .context("failed to signal pending plugin HTML to the bar")?;
        }
        if let Some(request) =
            active.filter(|request| self.overlay_generation == Some(request.generation))
            && self.latest.iter().any(|(key, render)| {
                !render.overlay_sent
                    && matches_flyout(key, request)
                    && persistent_ui_channel(SurfaceRole::Overlay, &key.2).is_some()
            })
        {
            emit(
                "plugin-ui-overlay",
                json!({ "generation": request.generation }),
            )
            .context("failed to signal pending plugin HTML to the open flyout")?;
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
