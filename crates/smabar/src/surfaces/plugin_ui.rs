//! Order flyout snapshots and live HTML with the existing native lifecycle.

use anyhow::Context;
use serde_json::Value;
use smabar_core::plugins::{PluginEvent, UiSnapshot};
use tauri::{AppHandle, Emitter, Manager};

use super::{OverlayFlyoutRequest, SurfaceManager, SurfaceRole};
use crate::commands::AppState;

impl SurfaceManager {
    /// Pending HTML for `role`, answered as a command response after a signal.
    pub(crate) fn take_plugin_ui(
        &self,
        app: &AppHandle,
        role: SurfaceRole,
    ) -> anyhow::Result<Vec<Value>> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        Ok(app.state::<AppState>().plugin_delivery.take(
            role,
            lifecycle
                .active_flyout
                .as_ref()
                .map(|active| &active.request),
        ))
    }

    /// One run of events becomes at most one signal per surface.
    pub(crate) fn deliver_plugin_ui(
        &self,
        app: &AppHandle,
        events: &[PluginEvent],
        suppress_events: bool,
    ) -> anyhow::Result<()> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        app.state::<AppState>().plugin_delivery.handle_many(
            events,
            lifecycle
                .active_flyout
                .as_ref()
                .map(|active| &active.request),
            |channel, payload| {
                // The no-events probe cuts transport, while retaining current
                // native snapshots for later opens and bar reloads.
                if suppress_events {
                    Ok(())
                } else {
                    app.emit(channel, payload)
                        .context("failed to emit plugin UI")
                }
            },
        )
    }

    pub(super) fn deliver_flyout(
        &self,
        app: &AppHandle,
        request: &OverlayFlyoutRequest,
        with_content: bool,
    ) -> anyhow::Result<()> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        // Staging a native window awaits the GUI thread. During that wait the
        // flyout can close, change tile, or pin without changing generation.
        if lifecycle
            .active_flyout
            .as_ref()
            .map(|active| &active.request)
            != Some(request)
        {
            return Ok(());
        }
        // Only enqueue prepared events while holding lifecycle → delivery.
        // Native creation, staging, focus and GUI getters must remain outside.
        app.state::<AppState>().plugin_delivery.open(
            request,
            with_content,
            |channel, payload| {
                app.emit(channel, payload)
                    .context("failed to emit flyout UI")
            },
        )?;
        if !with_content {
            // Same-generation promotion: notify the bar before releasing the
            // current-request guard, just like the overlay's pin request.
            app.emit("flyout-pinned", request)
                .context("failed to report pinned flyout to bar")?;
        }
        Ok(())
    }

    pub(crate) fn replay_plugin_ui(
        &self,
        app: &AppHandle,
        snapshot: Vec<UiSnapshot>,
    ) -> anyhow::Result<()> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let state = app.state::<AppState>();
        state.plugin_delivery.reset(snapshot);
        state.plugin_delivery.replay(
            lifecycle
                .active_flyout
                .as_ref()
                .map(|active| &active.request),
            |channel, payload| {
                app.emit(channel, payload)
                    .context("failed to replay plugin UI")
            },
        )
    }
}
