use std::time::Duration;

use anyhow::Context;
use smabar_core::config::{BarPosition, LayoutBehavior, SmabarConfig};
use smabar_core::platform::Rect;
use smabar_core::platform::surfaces::{ScreenRect, bar_surface_frame, offset_bar_surface};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use super::model::SurfaceSize;
use super::{SurfaceManager, SurfaceRole};
use crate::platform::strut::DockEdge;

const MOTION_TICK: Duration = Duration::from_millis(16);
const MAX_MOTION_MS: u32 = 2_000;
const MAX_VISIBLE_HEIGHT: u32 = 64;
/// A size slider reports many geometries per second; the reservation moves
/// every desktop window, so it follows once the reports stop.
const RESERVATION_SETTLE: Duration = Duration::from_millis(150);

impl SurfaceManager {
    /// Resizes and places the bar window at once (the live preview while a
    /// slider moves). Reservation and notification anchors follow through
    /// `reserve_bar_space` or `defer_bar_reservation`.
    pub(crate) fn update_bar_geometry(
        &self,
        window: &WebviewWindow,
        rect: Rect,
        surface: SurfaceSize,
        config: &SmabarConfig,
    ) -> anyhow::Result<ScreenRect> {
        let surface = surface.validate().map_err(anyhow::Error::msg)?;
        let monitor = self.active_monitor()?;
        let scale = monitor.scale_factor;
        let native_area = monitor.work_area;
        let area = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .bar_work_area
            .unwrap_or(native_area);
        let scaled_u32 = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
        let frame = bar_surface_frame(
            area,
            (scaled_u32(surface.width), scaled_u32(surface.height)),
            config.layout.position == BarPosition::Top,
        );
        let (frame_changed, offset, placed_frame) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.bar_geometry = Some((config.layout.position, rect));
            let changed = lifecycle.bar_surface_frame != Some(frame);
            lifecycle.bar_surface_frame = Some(frame);
            if changed {
                lifecycle.bar_motion_generation = lifecycle.bar_motion_generation.wrapping_add(1);
                lifecycle.bar_offset = target_offset(
                    frame,
                    monitor.frame,
                    config.layout.position,
                    scale,
                    config.layout.behavior,
                    lifecycle.bar_revealed || lifecycle.settings_group.is_some(),
                    lifecycle.bar_visible_height,
                );
            }
            let placed_frame = offset_bar_surface(
                frame,
                config.layout.position == BarPosition::Top,
                lifecycle.bar_offset,
            );
            lifecycle.bar_rect =
                (rect.w > 0 && rect.h > 0).then_some(bar_rect_in_frame(placed_frame, rect, scale));
            (changed, lifecycle.bar_offset, placed_frame)
        };
        if frame_changed {
            crate::platform::window::place_bar_surface(
                window,
                frame,
                tauri::PhysicalPosition::new(area.x, area.y),
                scale,
                config.layout.position,
            )?;
            crate::platform::window::move_bar_surface(
                window,
                frame,
                tauri::PhysicalPosition::new(area.x, area.y),
                scale,
                config.layout.position,
                offset,
                offset > 0,
            )?;
            tracing::info!(?frame, scale, "bar surface resized to content");
        }
        Ok(placed_frame)
    }

    /// Applies the reservation now; a pending deferred application is dropped.
    pub(crate) async fn reserve_bar_space(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        dock: Option<(DockEdge, Rect)>,
        frame: ScreenRect,
    ) -> anyhow::Result<()> {
        self.next_bar_reservation()?;
        self.apply_bar_reservation(app, window, dock, frame).await
    }

    /// Applies the reservation once the geometry reports settle; only the
    /// newest request survives.
    pub(crate) fn defer_bar_reservation(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        dock: Option<(DockEdge, Rect)>,
        frame: ScreenRect,
    ) -> anyhow::Result<()> {
        let generation = self.next_bar_reservation()?;
        let app = app.clone();
        let window = window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(RESERVATION_SETTLE).await;
            let manager = app.state::<SurfaceManager>();
            match manager.bar_reservation_is_current(generation) {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => {
                    tracing::error!(%error, "surface state unavailable; the settled bar reservation was not applied");
                    return;
                }
            }
            if let Err(error) = manager
                .apply_bar_reservation(&app, &window, dock, frame)
                .await
            {
                tracing::error!(%error, "failed to apply the settled bar reservation; resize the bar or switch the layout behavior to retry");
            }
        });
        Ok(())
    }

    async fn apply_bar_reservation(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        dock: Option<(DockEdge, Rect)>,
        frame: ScreenRect,
    ) -> anyhow::Result<()> {
        crate::platform::strut::apply(
            window,
            dock,
            Some(tauri::PhysicalPosition::new(frame.x, frame.y)),
        )?;
        let config = app.state::<crate::commands::AppState>().config();
        self.reposition_notifications(app, &config).await
    }

    fn next_bar_reservation(&self) -> anyhow::Result<u64> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        lifecycle.bar_reservation_generation = lifecycle.bar_reservation_generation.wrapping_add(1);
        Ok(lifecycle.bar_reservation_generation)
    }

    fn bar_reservation_is_current(&self, generation: u64) -> anyhow::Result<bool> {
        Ok(self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .bar_reservation_generation
            == generation)
    }

    pub(crate) async fn prepare_bar_relocation(
        &self,
        app: &AppHandle,
        position: BarPosition,
    ) -> anyhow::Result<()> {
        let (flyout, menu, tooltip) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
            let flyout = lifecycle.active_flyout.take().map(|active| active.request);
            let menu = lifecycle.active_menu.take().is_some();
            let tooltip = lifecycle.active_tooltip.take().is_some();
            lifecycle.flyout_layout = None;
            lifecycle.menu_frame = None;
            lifecycle.tooltip_layout = None;
            lifecycle.bar_geometry = None;
            lifecycle.bar_rect = None;
            lifecycle.bar_surface_frame = None;
            lifecycle.bar_relocation = Some(position);
            lifecycle.bar_relocation_frame = lifecycle.bar_relocation_frame.wrapping_add(1);
            lifecycle.bar_motion_generation = lifecycle.bar_motion_generation.wrapping_add(1);
            (flyout, menu, tooltip)
        };
        // An empty overlay is already being cleared or unmapped. Staging it
        // again would invalidate that pending clear without another DOM update.
        if flyout.is_some() || menu || tooltip {
            self.present_overlay(app).await?;
        }
        if let Some(overlay) = app.get_webview_window(SurfaceRole::Overlay.label()) {
            if let Some(closed) = flyout.as_ref() {
                overlay
                    .emit("flyout-closed", closed)
                    .context("failed to clear flyout before moving bar edge")?;
            }
            if menu {
                overlay
                    .emit("menu-closed", ())
                    .context("failed to clear context menu before moving bar edge")?;
            }
            if tooltip {
                overlay
                    .emit("tooltip-closed", ())
                    .context("failed to clear tooltip before moving bar edge")?;
            }
        }
        if let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label())
            && let Some(closed) = flyout
        {
            bar.emit("flyout-closed", closed)
                .context("failed to report flyout closed before moving bar edge")?;
        }
        Ok(())
    }

    pub(crate) fn begin_bar_relocation_frame(
        &self,
        position: BarPosition,
    ) -> anyhow::Result<Option<u64>> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        if lifecycle.bar_relocation != Some(position) {
            return Ok(None);
        }
        lifecycle.bar_relocation_frame = lifecycle.bar_relocation_frame.wrapping_add(1);
        Ok(Some(lifecycle.bar_relocation_frame))
    }

    pub(crate) fn claim_bar_relocation_frame(
        &self,
        position: BarPosition,
        frame: u64,
    ) -> anyhow::Result<bool> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let current =
            lifecycle.bar_relocation == Some(position) && lifecycle.bar_relocation_frame == frame;
        if current {
            lifecycle.bar_relocation = None;
        }
        Ok(current)
    }

    pub(crate) fn restore_bar_relocation(&self, position: BarPosition) -> anyhow::Result<()> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        lifecycle.bar_relocation = Some(position);
        lifecycle.bar_relocation_frame = lifecycle.bar_relocation_frame.wrapping_add(1);
        Ok(())
    }

    fn set_bar_revealed(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        config: &SmabarConfig,
        revealed: bool,
        duration_ms: u32,
        visible_height: u32,
    ) -> anyhow::Result<()> {
        if duration_ms > MAX_MOTION_MS {
            anyhow::bail!("autohide duration must not exceed {MAX_MOTION_MS} ms");
        }
        if visible_height == 0 || visible_height > MAX_VISIBLE_HEIGHT {
            anyhow::bail!(
                "autohide visible height must be between 1 and {MAX_VISIBLE_HEIGHT} pixels"
            );
        }
        let scale = window
            .scale_factor()
            .context("failed to read bar scale factor")?;
        let screen = self.active_monitor()?.frame;
        let (generation, frame, area, start, target) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.bar_revealed = config.layout.behavior != LayoutBehavior::Autohide || revealed;
            lifecycle.bar_visible_height = visible_height;
            lifecycle.bar_motion_generation = lifecycle.bar_motion_generation.wrapping_add(1);
            let Some(frame) = lifecycle.bar_surface_frame else {
                return Ok(());
            };
            let Some(area) = lifecycle.bar_work_area else {
                return Ok(());
            };
            let target = target_offset(
                frame,
                screen,
                config.layout.position,
                scale,
                config.layout.behavior,
                lifecycle.bar_revealed || lifecycle.settings_group.is_some(),
                lifecycle.bar_visible_height,
            );
            (
                lifecycle.bar_motion_generation,
                frame,
                area,
                lifecycle.bar_offset,
                target,
            )
        };
        if start == target || duration_ms == 0 {
            self.lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .bar_offset = target;
            return crate::platform::window::move_bar_surface(
                window,
                frame,
                tauri::PhysicalPosition::new(area.x, area.y),
                scale,
                config.layout.position,
                target,
                target > 0,
            );
        }

        let handle = app.clone();
        let moved = window.clone();
        let position = config.layout.position;
        tauri::async_runtime::spawn(async move {
            let started = tokio::time::Instant::now();
            loop {
                let elapsed = started.elapsed().as_millis().min(u128::from(duration_ms)) as u32;
                let offset = motion_offset(start, target, elapsed, duration_ms);
                let manager = handle.state::<SurfaceManager>();
                let current = match manager.lifecycle.lock() {
                    Ok(mut lifecycle) if lifecycle.bar_motion_generation == generation => {
                        lifecycle.bar_offset = offset;
                        true
                    }
                    Ok(_) => false,
                    Err(_) => {
                        tracing::error!(
                            "surface lifecycle lock poisoned; native autohide animation stopped"
                        );
                        false
                    }
                };
                if !current {
                    return;
                }
                if let Err(error) = crate::platform::window::move_bar_surface(
                    &moved,
                    frame,
                    tauri::PhysicalPosition::new(area.x, area.y),
                    scale,
                    position,
                    offset,
                    offset == target && target > 0,
                ) {
                    tracing::error!(
                        %error,
                        "failed to move the native autohide surface; toggle autohide to retry"
                    );
                    return;
                }
                if elapsed == duration_ms {
                    return;
                }
                tokio::time::sleep(MOTION_TICK).await;
            }
        });
        Ok(())
    }
}

fn bar_rect_in_frame(frame: ScreenRect, rect: Rect, scale: f64) -> ScreenRect {
    let scaled_i32 = |value: i32| (f64::from(value) * scale).round() as i32;
    let scaled_u32 = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
    ScreenRect {
        x: frame.x.saturating_add(scaled_i32(rect.x)),
        y: frame.y.saturating_add(scaled_i32(rect.y)),
        w: scaled_u32(rect.w),
        h: scaled_u32(rect.h),
    }
}

fn target_offset(
    frame: ScreenRect,
    screen: ScreenRect,
    position: BarPosition,
    scale: f64,
    behavior: LayoutBehavior,
    revealed: bool,
    visible_height: u32,
) -> u32 {
    if behavior != LayoutBehavior::Autohide || revealed {
        return 0;
    }
    let visible = (f64::from(visible_height) * scale).round().max(1.0) as u32;
    let distance = if position == BarPosition::Top {
        i64::from(frame.y) + i64::from(frame.h) - i64::from(screen.y)
    } else {
        i64::from(screen.y) + i64::from(screen.h) - i64::from(frame.y)
    };
    (distance - i64::from(visible)).clamp(0, i64::from(u32::MAX)) as u32
}

fn motion_offset(start: u32, target: u32, elapsed_ms: u32, duration_ms: u32) -> u32 {
    if duration_ms == 0 || elapsed_ms >= duration_ms {
        return target;
    }
    let progress = f64::from(elapsed_ms) / f64::from(duration_ms);
    let eased = 1.0 - (1.0 - progress).powi(3);
    (f64::from(start) + (f64::from(target) - f64::from(start)) * eased).round() as u32
}

#[tauri::command]
pub fn set_bar_revealed(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, crate::commands::AppState>,
    manager: State<'_, SurfaceManager>,
    revealed: bool,
    duration_ms: u32,
    visible_height: u32,
) -> Result<(), String> {
    if window.label() != SurfaceRole::Bar.label() {
        return Err("autohide state can only originate from the bar surface".to_string());
    }
    manager
        .set_bar_revealed(
            &app,
            &window,
            &state.config(),
            revealed,
            duration_ms,
            visible_height,
        )
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests;
