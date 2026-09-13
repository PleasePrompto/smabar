use anyhow::Context;
use smabar_core::platform::surfaces::{FlyoutDirection, union};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

use super::model::{FlyoutMode, MenuPlacement, OverlayPlacement, TooltipPlacement};
use super::{SurfaceManager, SurfaceRole};

/// Conceals the native surface before React replaces its contents. X11 keeps
/// it mapped at zero opacity; Wayland withdraws it until the replacement is placed.
pub(super) async fn stage_transient_update(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    crate::platform::stage_transient_update(window).await
}

impl SurfaceManager {
    pub(super) async fn present_overlay(&self, app: &AppHandle) -> anyhow::Result<()> {
        let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) else {
            return Ok(());
        };
        // Captured before reading the state below: any content change staged
        // after this read invalidates the token, so the reveal at the end of
        // this presentation cannot expose a superseded WebKit buffer.
        let token = crate::platform::transient_presentation_token(&window)?;
        let (flyout, flyout_layout, menu, menu_frame, tooltip, tooltip_layout) = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            (
                lifecycle.active_flyout.clone(),
                lifecycle.flyout_layout,
                lifecycle.active_menu.clone(),
                lifecycle.menu_frame,
                lifecycle.active_tooltip.clone(),
                lifecycle.tooltip_layout,
            )
        };
        let frame = [
            flyout_layout.map(|layout| layout.frame),
            menu_frame,
            tooltip_layout.map(|layout| layout.frame),
        ]
        .into_iter()
        .flatten()
        .reduce(union);
        let Some(frame) = frame else {
            self.lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .overlay_frame = None;
            stage_transient_update(&window).await?;
            return Ok(());
        };
        let monitor = self.active_monitor()?;
        let native_area = monitor.work_area;
        let scale = monitor.scale_factor;
        tracing::debug!(
            generation = self.overlay_generation()?,
            ?frame,
            scale,
            "placing overlay surface"
        );
        crate::platform::window::place_surface(
            &window,
            SurfaceRole::Overlay,
            PhysicalPosition::new(frame.x, frame.y),
            PhysicalSize::new(frame.w, frame.h),
            PhysicalPosition::new(native_area.x, native_area.y),
            scale,
            menu.is_some()
                || flyout
                    .as_ref()
                    .is_some_and(|active| active.request.mode == FlyoutMode::Pinned),
        )
        .await?;
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .overlay_frame = Some(frame);
        if let (Some(active), Some(layout)) = (flyout.as_ref(), flyout_layout) {
            window
                .emit(
                    "overlay-placement",
                    OverlayPlacement {
                        generation: active.request.generation,
                        direction: direction_name(layout.direction),
                        pointer_x: f64::from(layout.pointer_x) / scale,
                        x: f64::from(layout.frame.x - frame.x) / scale,
                        y: f64::from(layout.frame.y - frame.y) / scale,
                    },
                )
                .context("failed to send flyout placement")?;
        }
        if let (Some(active), Some(_)) = (menu.as_ref(), menu_frame) {
            window
                .emit(
                    "menu-placement",
                    MenuPlacement {
                        generation: active.request.generation,
                        anchor_x: f64::from(active.anchor.x - frame.x) / scale,
                        anchor_y: f64::from(active.anchor.y - frame.y) / scale,
                    },
                )
                .context("failed to send menu placement")?;
        }
        if let (Some(active), Some(layout)) = (tooltip.as_ref(), tooltip_layout) {
            window
                .emit(
                    "tooltip-placement",
                    TooltipPlacement {
                        generation: active.request.generation,
                        side: tooltip_side(layout.direction),
                        x: f64::from(layout.frame.x - frame.x) / scale,
                        y: f64::from(layout.frame.y - frame.y) / scale,
                    },
                )
                .context("failed to send tooltip placement")?;
        }
        let interactive = menu.is_some() || flyout.is_some();
        // Tao's Linux cursor-routing request expects a mapped GDK window.
        crate::platform::present_transient(&window, token).context("failed to present overlay")?;
        if let Some(active) = flyout.as_ref() {
            crate::memory_probe::surface(
                app,
                "present-scheduled",
                active.request.generation,
                Some(&active.request.tile_id),
            );
        }
        window
            .set_ignore_cursor_events(!interactive)
            .context("failed to update overlay pointer routing")?;
        if menu.is_some() || flyout.is_some_and(|active| active.request.mode == FlyoutMode::Pinned)
        {
            crate::platform::window::focus(&window).context("failed to focus overlay")?;
        }
        Ok(())
    }

    fn finalize_overlay_clear(&self, app: &AppHandle) -> anyhow::Result<()> {
        let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) else {
            return Ok(());
        };
        // Captured before the emptiness check: content opened after the check
        // stages first, which invalidates this token and aborts the unmap.
        // Unmapping a WebKit surface that is about to show replacement content
        // stalls its accelerated buffer and can resurrect the previous frame.
        let token = crate::platform::transient_presentation_token(&window)?;
        let (empty, clear_generation) = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            (
                lifecycle.active_flyout.is_none()
                    && lifecycle.active_menu.is_none()
                    && lifecycle.active_tooltip.is_none(),
                lifecycle.overlay_generation,
            )
        };
        if !empty {
            return Ok(());
        }
        crate::platform::hide_transient_after_paint(&window, token)
            .context("failed to hide cleared overlay")?;
        // A newer open can invalidate the queued hide; log the generation
        // whose empty state authorized it, not a later lifecycle snapshot.
        crate::memory_probe::surface(app, "clear-scheduled", clear_generation, None);
        Ok(())
    }
}

#[tauri::command]
pub fn finalize_overlay_clear(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
) -> Result<(), String> {
    if window.label() != SurfaceRole::Overlay.label() {
        return Err("only the overlay surface can finalize its clear".to_string());
    }
    manager
        .finalize_overlay_clear(&app)
        .map_err(|error| format!("{error:#}"))
}

fn direction_name(direction: FlyoutDirection) -> &'static str {
    match direction {
        FlyoutDirection::Up => "up",
        FlyoutDirection::Down => "down",
    }
}

fn tooltip_side(direction: FlyoutDirection) -> &'static str {
    match direction {
        FlyoutDirection::Up => "top",
        FlyoutDirection::Down => "bottom",
    }
}
