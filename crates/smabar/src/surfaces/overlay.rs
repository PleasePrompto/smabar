use anyhow::Context;
use smabar_core::config::BarPosition;
use smabar_core::platform::Rect;
use smabar_core::platform::surfaces::{ScreenRect, flyout_frame};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use super::model::{ActiveFlyout, FlyoutLayout, FlyoutMode, OverlayFlyoutRequest, OverlayMeasure};
use super::{SurfaceManager, SurfaceRole};
use crate::commands::AppState;

impl SurfaceManager {
    async fn open_flyout(
        &self,
        app: &AppHandle,
        source: &WebviewWindow,
        tile_id: String,
        mode: FlyoutMode,
        trigger: Rect,
    ) -> anyhow::Result<()> {
        validate_tile_id(&tile_id)?;
        validate_rect(trigger)?;
        let promote_generation = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            peek_to_promote(lifecycle.active_flyout.as_ref(), &tile_id, mode)
                .filter(|_| lifecycle.flyout_layout.is_some())
        };
        if let Some(generation) = promote_generation {
            // Only the overlay knows which HTML is actually on screen.
            // Let it compare the preview with the pinned content before staging.
            if let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) {
                window
                    .emit("flyout-pin-requested", generation)
                    .context("failed to request flyout promotion")?;
            }
            return Ok(());
        }
        let trigger = self.rect_on_screen(source, trigger)?;
        let (request, tooltip_closed) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
            let request = OverlayFlyoutRequest {
                generation: lifecycle.overlay_generation,
                tile_id,
                mode,
                preserve_content: false,
            };
            lifecycle.active_flyout = Some(ActiveFlyout {
                request: request.clone(),
                trigger,
            });
            lifecycle.flyout_layout = None;
            let tooltip_closed = lifecycle.active_tooltip.take().is_some();
            lifecycle.tooltip_layout = None;
            (request, tooltip_closed)
        };
        let ready = self.is_ready(SurfaceRole::Overlay)?;
        let window = self.ensure(app, SurfaceRole::Overlay, None)?;
        crate::memory_probe::surface(app, "open", request.generation, Some(&request.tile_id));
        tracing::debug!(
            generation = request.generation,
            tile = %request.tile_id,
            mode = ?request.mode,
            "opening flyout"
        );
        super::presentation::stage_transient_update(&window).await?;
        crate::memory_probe::surface(app, "staged", request.generation, Some(&request.tile_id));
        tracing::debug!(generation = request.generation, "flyout staged");
        if tooltip_closed {
            window
                .emit("tooltip-closed", ())
                .context("failed to clear tooltip before opening flyout")?;
        }
        if ready {
            self.deliver_flyout(app, &request, true)?;
        }
        Ok(())
    }

    async fn measure_flyout(
        &self,
        app: &AppHandle,
        measure: OverlayMeasure,
        bar_position: BarPosition,
    ) -> anyhow::Result<()> {
        validate_measure(measure)?;
        let active = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle
                .active_flyout
                .as_ref()
                .filter(|active| active.request.generation == measure.generation)
                .cloned()
        };
        let Some(active) = active else {
            tracing::debug!(
                generation = measure.generation,
                "ignored stale overlay measurement"
            );
            return Ok(());
        };
        let monitor = self.active_monitor()?;
        let area = monitor.work_area;
        let scale = monitor.scale_factor;
        let scaled = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
        let placement = flyout_frame(
            area,
            active.trigger,
            (scaled(measure.width), scaled(measure.height)),
            bar_position == BarPosition::Top,
            scaled(measure.inset),
            scaled(measure.gap),
            scaled(measure.pointer_reserve),
        );
        {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            // Re-checked under the write lock: a flyout opened while this
            // measurement was computing must not receive the old layout, or
            // the presentation would reveal the superseded WebKit buffer.
            if lifecycle
                .active_flyout
                .as_ref()
                .map(|active| active.request.generation)
                != Some(measure.generation)
            {
                tracing::debug!(
                    generation = measure.generation,
                    "discarded superseded overlay layout"
                );
                return Ok(());
            }
            let layout = FlyoutLayout {
                frame: placement.frame,
                direction: placement.direction,
                pointer_x: placement.pointer_x,
            };
            // A plugin pushing new content into its open flyout re-measures
            // it; the same box needs no second place/reveal chain (that chain
            // is a WebKit snapshot plus forced paints, and on Windows a
            // hide/show). Every path that needs a fresh presentation clears
            // the layout first (open, content-changing pin, close).
            if lifecycle.flyout_layout == Some(layout) {
                tracing::debug!(
                    generation = measure.generation,
                    "overlay layout unchanged; skipping re-presentation"
                );
                return Ok(());
            }
            lifecycle.flyout_layout = Some(layout);
        }
        self.present_overlay(app).await
    }

    async fn pin_flyout(
        &self,
        app: &AppHandle,
        generation: u64,
        replace_content: bool,
    ) -> anyhow::Result<()> {
        let request = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            let preserve_content = !replace_content && lifecycle.flyout_layout.is_some();
            let Some(active) = lifecycle.active_flyout.as_mut().filter(|active| {
                active.request.generation == generation && active.request.mode == FlyoutMode::Peek
            }) else {
                return Ok(());
            };
            active.request.mode = FlyoutMode::Pinned;
            active.request.preserve_content = preserve_content;
            let request = active.request.clone();
            if !preserve_content {
                lifecycle.flyout_layout = None;
            }
            request
        };
        if let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) {
            if request.preserve_content {
                crate::platform::window::focus(&window).context("failed to focus pinned flyout")?;
            } else {
                super::presentation::stage_transient_update(&window).await?;
            }
            tracing::debug!(
                generation,
                preserve_content = request.preserve_content,
                "pinning flyout"
            );
            self.deliver_flyout(app, &request, false)?;
        }
        Ok(())
    }

    async fn close_flyout(&self, app: &AppHandle, generation: Option<u64>) -> anyhow::Result<()> {
        let closed = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            if generation.is_some_and(|expected| {
                lifecycle
                    .active_flyout
                    .as_ref()
                    .map(|active| active.request.generation)
                    != Some(expected)
            }) {
                return Ok(());
            }
            lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
            lifecycle.flyout_layout = None;
            lifecycle.active_flyout.take().map(|active| active.request)
        };
        let Some(closed) = closed else {
            return Ok(());
        };
        crate::memory_probe::surface(app, "close", closed.generation, Some(&closed.tile_id));
        self.present_overlay(app).await?;
        if let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) {
            bar.emit("flyout-closed", closed.clone())
                .context("failed to report closed flyout to bar")?;
        }
        if let Some(overlay) = app.get_webview_window(SurfaceRole::Overlay.label()) {
            overlay
                .emit("flyout-closed", closed)
                .context("failed to clear closed flyout content")?;
        }
        Ok(())
    }

    fn report_pointer(&self, app: &AppHandle, inside: bool) -> anyhow::Result<()> {
        if let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) {
            bar.emit("overlay-pointer", inside)
                .context("failed to report overlay pointer state")?;
        }
        Ok(())
    }
}

fn peek_to_promote(active: Option<&ActiveFlyout>, tile_id: &str, mode: FlyoutMode) -> Option<u64> {
    active
        .filter(|active| {
            mode == FlyoutMode::Pinned
                && active.request.mode == FlyoutMode::Peek
                && active.request.tile_id == tile_id
        })
        .map(|active| active.request.generation)
}

impl SurfaceManager {
    pub(super) fn rect_on_screen(
        &self,
        window: &WebviewWindow,
        rect: Rect,
    ) -> anyhow::Result<ScreenRect> {
        let monitor = self.active_monitor()?;
        let scale = monitor.scale_factor;
        let origin = self.trigger_origin(window)?;
        tracing::debug!(
            surface = window.label(),
            ?rect,
            ?origin,
            scale,
            "mapping surface trigger to screen"
        );
        let scaled = |value: i32| (f64::from(value) * scale).round() as i32;
        Ok(ScreenRect {
            x: origin.x.saturating_add(scaled(rect.x)),
            y: origin.y.saturating_add(scaled(rect.y)),
            w: (f64::from(rect.w) * scale).round().max(1.0) as u32,
            h: (f64::from(rect.h) * scale).round().max(1.0) as u32,
        })
    }
}

pub(super) fn validate_rect(rect: Rect) -> anyhow::Result<()> {
    if rect.w == 0 || rect.h == 0 || rect.w > 16_384 || rect.h > 16_384 {
        anyhow::bail!("flyout trigger must have a visible bounded size");
    }
    Ok(())
}

fn validate_tile_id(tile_id: &str) -> anyhow::Result<()> {
    if !tile_id.starts_with("plugin:") || tile_id.len() > 512 {
        anyhow::bail!("flyout tile id must be a namespaced plugin tile");
    }
    Ok(())
}

fn validate_measure(measure: OverlayMeasure) -> anyhow::Result<()> {
    if measure.width == 0
        || measure.height == 0
        || measure.width > 16_384
        || measure.height > 16_384
    {
        anyhow::bail!("overlay size must be between 1 and 16384 CSS pixels");
    }
    if measure.inset > 512 || measure.gap > 512 || measure.pointer_reserve > 512 {
        anyhow::bail!("overlay spacing must not exceed 512 CSS pixels");
    }
    Ok(())
}

#[tauri::command]
pub async fn open_flyout(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
    tile_id: String,
    mode: FlyoutMode,
    trigger: Rect,
) -> Result<(), String> {
    if !matches!(
        SurfaceRole::from_label(window.label()),
        Some(SurfaceRole::Bar | SurfaceRole::Overlay)
    ) {
        return Err("flyouts can only originate from the bar or overlay".to_string());
    }
    manager
        .open_flyout(&app, &window, tile_id, mode, trigger)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn measure_flyout(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, SurfaceManager>,
    measure: OverlayMeasure,
) -> Result<(), String> {
    manager
        .measure_flyout(&app, measure, state.config().layout.position)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn pin_flyout(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    generation: u64,
    replace_content: bool,
) -> Result<(), String> {
    manager
        .pin_flyout(&app, generation, replace_content)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn close_flyout(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    generation: Option<u64>,
) -> Result<(), String> {
    manager
        .close_flyout(&app, generation)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn set_overlay_pointer(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    inside: bool,
) -> Result<(), String> {
    manager
        .report_pointer(&app, inside)
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests;
