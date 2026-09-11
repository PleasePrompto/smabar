use anyhow::Context;
use smabar_core::config::BarPosition;
use smabar_core::platform::Rect;
use smabar_core::platform::surfaces::{ScreenRect, tooltip_frame};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use super::model::{ActiveTooltip, OverlayTooltipRequest, TooltipLayout, TooltipMeasure};
use super::overlay::validate_rect;
use super::{SurfaceManager, SurfaceRole};
use crate::commands::AppState;

const MAX_TOOLTIP_CHARS: usize = 2_048;

impl SurfaceManager {
    async fn open_tooltip(
        &self,
        app: &AppHandle,
        source: &WebviewWindow,
        text: String,
        trigger: Rect,
    ) -> anyhow::Result<()> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_TOOLTIP_CHARS {
            anyhow::bail!("tooltip text must contain between 1 and 2048 characters");
        }
        validate_rect(trigger)?;
        let trigger = self.rect_on_screen(source, trigger)?;
        let (request, replacing) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            if lifecycle.active_flyout.is_some() || lifecycle.active_menu.is_some() {
                return Ok(());
            }
            let generation =
                matching_tooltip_generation(lifecycle.active_tooltip.as_ref(), trigger);
            let replacing = generation.is_none();
            if replacing {
                lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
                lifecycle.tooltip_layout = None;
            }
            let request = OverlayTooltipRequest {
                generation: generation.unwrap_or(lifecycle.overlay_generation),
                text: text.to_owned(),
            };
            lifecycle.active_tooltip = Some(ActiveTooltip {
                request: request.clone(),
                trigger,
            });
            (request, replacing)
        };
        let ready = self.is_ready(SurfaceRole::Overlay)?;
        let window = self.ensure(app, SurfaceRole::Overlay, None)?;
        if replacing {
            super::presentation::stage_transient_update(&window).await?;
        }
        if ready {
            window
                .emit("surface-tooltip", request)
                .context("failed to deliver tooltip")?;
        }
        Ok(())
    }

    async fn measure_tooltip(
        &self,
        app: &AppHandle,
        measure: TooltipMeasure,
        bar_position: BarPosition,
    ) -> anyhow::Result<()> {
        validate_measure(measure)?;
        let active = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle
                .active_tooltip
                .as_ref()
                .filter(|active| active.request.generation == measure.generation)
                .cloned()
        };
        let Some(active) = active else {
            return Ok(());
        };
        let monitor = self.active_monitor()?;
        let area = monitor.work_area;
        let scale = monitor.scale_factor;
        let scaled = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
        let placed = tooltip_frame(
            area,
            active.trigger,
            (scaled(measure.width), scaled(measure.height)),
            bar_position == BarPosition::Top,
            scaled(measure.inset),
            scaled(measure.gap),
        );
        {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            // Re-checked under the write lock so a tooltip replaced during the
            // computation cannot present with the superseded layout.
            if lifecycle
                .active_tooltip
                .as_ref()
                .map(|active| active.request.generation)
                != Some(measure.generation)
            {
                return Ok(());
            }
            lifecycle.tooltip_layout = Some(TooltipLayout {
                frame: placed.frame,
                direction: placed.direction,
            });
        }
        self.present_overlay(app).await
    }

    async fn close_tooltip(&self, app: &AppHandle, generation: Option<u64>) -> anyhow::Result<()> {
        let closed = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            if generation.is_some_and(|expected| {
                lifecycle
                    .active_tooltip
                    .as_ref()
                    .map(|active| active.request.generation)
                    != Some(expected)
            }) {
                return Ok(());
            }
            lifecycle.tooltip_layout = None;
            lifecycle.active_tooltip.take().is_some()
        };
        if !closed {
            return Ok(());
        }
        self.present_overlay(app).await?;
        if let Some(window) = app.get_webview_window(SurfaceRole::Overlay.label()) {
            window
                .emit("tooltip-closed", ())
                .context("failed to clear tooltip")?;
        }
        Ok(())
    }
}

fn matching_tooltip_generation(active: Option<&ActiveTooltip>, trigger: ScreenRect) -> Option<u64> {
    active
        .filter(|active| active.trigger == trigger)
        .map(|active| active.request.generation)
}

fn validate_measure(measure: TooltipMeasure) -> anyhow::Result<()> {
    if measure.width == 0
        || measure.height == 0
        || measure.width > 16_384
        || measure.height > 16_384
    {
        anyhow::bail!("tooltip size must be between 1 and 16384 CSS pixels");
    }
    if measure.inset > 512 || measure.gap > 512 {
        anyhow::bail!("tooltip spacing must not exceed 512 CSS pixels");
    }
    Ok(())
}

#[tauri::command]
pub async fn open_tooltip(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
    text: String,
    trigger: Rect,
) -> Result<(), String> {
    if !matches!(
        SurfaceRole::from_label(window.label()),
        Some(SurfaceRole::Bar | SurfaceRole::Overlay)
    ) {
        return Err("tooltips can only originate from the bar or overlay".to_string());
    }
    manager
        .open_tooltip(&app, &window, text, trigger)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn measure_tooltip(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, SurfaceManager>,
    measure: TooltipMeasure,
) -> Result<(), String> {
    manager
        .measure_tooltip(&app, measure, state.config().layout.position)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn close_tooltip(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    generation: Option<u64>,
) -> Result<(), String> {
    manager
        .close_tooltip(&app, generation)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_updates_to_the_same_anchor_reuse_the_tooltip_generation() {
        let trigger = ScreenRect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };
        let active = ActiveTooltip {
            request: OverlayTooltipRequest {
                generation: 7,
                text: "CPU 5%".to_string(),
            },
            trigger,
        };

        assert_eq!(matching_tooltip_generation(Some(&active), trigger), Some(7));
        assert_eq!(
            matching_tooltip_generation(Some(&active), ScreenRect { x: 11, ..trigger }),
            None
        );
    }
}
