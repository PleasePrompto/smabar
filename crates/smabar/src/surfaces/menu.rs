use anyhow::Context;
use serde_json::Value;
use smabar_core::platform::surfaces::{ScreenPoint, menu_frame};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use super::model::{ActiveMenu, ClientPoint, MenuMeasure, OverlayMenuRequest};
use super::{SurfaceManager, SurfaceRole};

const MAX_MENU_BYTES: usize = 64 * 1024;

impl SurfaceManager {
    async fn open_menu(
        &self,
        app: &AppHandle,
        source: &WebviewWindow,
        items: Value,
        anchor: ClientPoint,
        keep_flyout: bool,
    ) -> anyhow::Result<()> {
        validate_items(&items)?;
        let anchor = self.point_on_screen(source, anchor)?;
        let (request, closed_flyout, tooltip_closed) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            let closed = if keep_flyout {
                None
            } else {
                lifecycle.flyout_layout = None;
                lifecycle.active_flyout.take().map(|active| active.request)
            };
            lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
            let request = OverlayMenuRequest {
                generation: lifecycle.overlay_generation,
                items,
            };
            lifecycle.active_menu = Some(ActiveMenu {
                request: request.clone(),
                anchor,
            });
            lifecycle.menu_frame = None;
            let tooltip_closed = lifecycle.active_tooltip.take().is_some();
            lifecycle.tooltip_layout = None;
            (request, closed, tooltip_closed)
        };
        let ready = self.is_ready(SurfaceRole::Overlay)?;
        let window = self.ensure(app, SurfaceRole::Overlay, None)?;
        super::presentation::stage_transient_update(&window).await?;
        if let Some(closed) = closed_flyout {
            if let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) {
                bar.emit("flyout-closed", closed.clone())
                    .context("failed to report flyout closed by context menu")?;
            }
            window
                .emit("flyout-closed", closed)
                .context("failed to clear flyout content behind context menu")?;
        }
        if tooltip_closed {
            window
                .emit("tooltip-closed", ())
                .context("failed to clear tooltip before opening context menu")?;
        }
        if ready {
            window
                .emit("surface-menu", request)
                .context("failed to deliver context menu")?;
        }
        Ok(())
    }

    async fn measure_menu(&self, app: &AppHandle, measure: MenuMeasure) -> anyhow::Result<()> {
        validate_measure(measure)?;
        let active = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle
                .active_menu
                .as_ref()
                .filter(|active| active.request.generation == measure.generation)
                .cloned()
        };
        let Some(active) = active else {
            tracing::debug!(
                generation = measure.generation,
                "ignored stale menu measurement"
            );
            return Ok(());
        };
        let monitor = self.active_monitor()?;
        let area = monitor.work_area;
        let scale = monitor.scale_factor;
        let scaled = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
        let frame = menu_frame(
            area,
            active.anchor,
            (scaled(measure.width), scaled(measure.height)),
            scaled(measure.inset),
        );
        {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            // Re-checked under the write lock so a menu replaced during the
            // computation cannot present with the superseded frame.
            if lifecycle
                .active_menu
                .as_ref()
                .map(|active| active.request.generation)
                != Some(measure.generation)
            {
                return Ok(());
            }
            lifecycle.menu_frame = Some(frame);
        }
        self.present_overlay(app).await
    }

    async fn close_menu(&self, app: &AppHandle, generation: Option<u64>) -> anyhow::Result<()> {
        let remaining_overlay = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            if generation.is_some_and(|expected| {
                lifecycle
                    .active_menu
                    .as_ref()
                    .map(|active| active.request.generation)
                    != Some(expected)
            }) {
                return Ok(());
            }
            lifecycle.active_menu = None;
            lifecycle.menu_frame = None;
            lifecycle.active_flyout.is_some()
        };
        self.present_overlay(app).await?;
        if let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) {
            window
                .emit("menu-closed", ())
                .context("failed to clear context menu")?;
        }
        if !remaining_overlay && let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) {
            crate::platform::window::focus(&bar).context("failed to return focus to bar")?;
        }
        Ok(())
    }

    fn point_on_screen(
        &self,
        window: &WebviewWindow,
        point: ClientPoint,
    ) -> anyhow::Result<ScreenPoint> {
        let monitor = self.active_monitor()?;
        let scale = monitor.scale_factor;
        let origin = self.trigger_origin(window)?;
        let screen = ScreenPoint {
            x: origin
                .x
                .saturating_add((f64::from(point.x) * scale).round() as i32),
            y: origin
                .y
                .saturating_add((f64::from(point.y) * scale).round() as i32),
        };
        tracing::debug!(
            surface = window.label(),
            ?point,
            ?origin,
            scale,
            ?screen,
            "resolved context-menu anchor"
        );
        Ok(screen)
    }
}

fn validate_items(items: &Value) -> anyhow::Result<()> {
    if !items.is_array() {
        anyhow::bail!("context menu items must be an array");
    }
    if serde_json::to_vec(items)?.len() > MAX_MENU_BYTES {
        anyhow::bail!("context menu payload exceeds 64 KiB");
    }
    Ok(())
}

fn validate_measure(measure: MenuMeasure) -> anyhow::Result<()> {
    if measure.width == 0
        || measure.height == 0
        || measure.width > 16_384
        || measure.height > 16_384
    {
        anyhow::bail!("menu size must be between 1 and 16384 CSS pixels");
    }
    if measure.inset > 512 {
        anyhow::bail!("menu inset must not exceed 512 CSS pixels");
    }
    Ok(())
}

#[tauri::command]
pub async fn open_context_menu(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
    items: Value,
    anchor: ClientPoint,
    keep_flyout: bool,
) -> Result<(), String> {
    if !matches!(
        SurfaceRole::from_label(window.label()),
        Some(SurfaceRole::Bar | SurfaceRole::Overlay)
    ) {
        return Err("context menus can only originate from the bar or overlay".to_string());
    }
    manager
        .open_menu(&app, &window, items, anchor, keep_flyout)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn measure_context_menu(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    measure: MenuMeasure,
) -> Result<(), String> {
    manager
        .measure_menu(&app, measure)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn close_context_menu(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    generation: Option<u64>,
) -> Result<(), String> {
    manager
        .close_menu(&app, generation)
        .await
        .map_err(|error| format!("{error:#}"))
}
