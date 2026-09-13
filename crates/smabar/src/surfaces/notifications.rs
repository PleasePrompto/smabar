use anyhow::Context;
use serde_json::Value;
use smabar_core::config::{BarPosition, PopupPosition, SmabarConfig};
use smabar_core::platform::surfaces::{
    HorizontalAnchor, ScreenRect, VerticalAnchor, anchored_frame, union,
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State};

use super::model::{LocalPoint, NoticeRequest, NotificationMeasure, NotificationPlacement};
use super::{SurfaceManager, SurfaceRole};
use crate::commands::AppState;

impl SurfaceManager {
    pub(crate) fn prepare_notifications(&self, app: &AppHandle) -> anyhow::Result<()> {
        self.ensure(app, SurfaceRole::Notifications, None)
            .map(|_| ())
    }

    pub(crate) async fn enqueue_popup(
        &self,
        app: &AppHandle,
        payload: Value,
    ) -> anyhow::Result<()> {
        let ready = self.is_ready(SurfaceRole::Notifications)?;
        let window = self.ensure(app, SurfaceRole::Notifications, None)?;
        if ready {
            super::presentation::stage_transient_update(&window).await?;
            window
                .emit("plugin-ui", payload)
                .context("failed to deliver plugin popup")?;
        } else {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            if lifecycle.pending_popups.len() >= 50 {
                let dropped = lifecycle.pending_popups.remove(0);
                tracing::warn!(
                    plugin = dropped.get("pluginId").and_then(serde_json::Value::as_str),
                    "popup startup queue is full; oldest waiting notification discarded"
                );
            }
            lifecycle.pending_popups.push(payload);
        }
        Ok(())
    }

    async fn show_notice(&self, app: &AppHandle, notice: NoticeRequest) -> anyhow::Result<()> {
        let ready = self.is_ready(SurfaceRole::Notifications)?;
        let window = self.ensure(app, SurfaceRole::Notifications, None)?;
        if ready {
            super::presentation::stage_transient_update(&window).await?;
            window
                .emit("surface-notice", notice)
                .context("failed to deliver shell notice")?;
        } else {
            self.lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .pending_notice = Some(notice);
        }
        Ok(())
    }

    async fn set_notification_measure(
        &self,
        app: &AppHandle,
        measure: NotificationMeasure,
        config: &SmabarConfig,
    ) -> anyhow::Result<()> {
        if let Some(size) = measure.popup {
            size.validate().map_err(anyhow::Error::msg)?;
        }
        if let Some(size) = measure.notice {
            size.validate().map_err(anyhow::Error::msg)?;
        }
        if measure.edge_inset > 512 || measure.gap > 512 {
            anyhow::bail!("notification spacing must not exceed 512 CSS pixels");
        }
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .notification_measure = Some(measure);
        self.reposition_notifications(app, config).await
    }

    pub(crate) async fn reposition_notifications(
        &self,
        app: &AppHandle,
        config: &SmabarConfig,
    ) -> anyhow::Result<()> {
        let Some(window) = app.get_webview_window(SurfaceRole::Notifications.label()) else {
            return Ok(());
        };
        let (measure, bar_rect) = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            (lifecycle.notification_measure, lifecycle.bar_rect)
        };
        let Some(measure) = measure else {
            return Ok(());
        };
        let monitor = self.active_monitor()?;
        let area = monitor.work_area;
        let scale = monitor.scale_factor;
        let scaled = |value: u32| (f64::from(value) * scale).round().max(1.0) as u32;
        let inset = scaled(measure.edge_inset);
        let gap = scaled(measure.gap);
        let (horizontal, vertical) = popup_anchors(config.popups.position);
        let popup = measure.popup.map(|size| {
            anchored_frame(
                area,
                (scaled(size.width), scaled(size.height)),
                horizontal,
                vertical,
                inset,
                bar_on_edge(bar_rect, config.layout.position, vertical),
                gap,
            )
        });
        let notice_vertical = match config.layout.position {
            BarPosition::Top => VerticalAnchor::Top,
            BarPosition::Bottom => VerticalAnchor::Bottom,
        };
        let notice = measure.notice.map(|size| {
            anchored_frame(
                area,
                (scaled(size.width), scaled(size.height)),
                HorizontalAnchor::Center,
                notice_vertical,
                inset,
                bar_rect,
                gap,
            )
        });
        let frame = match (popup, notice) {
            (Some(popup), Some(notice)) => union(popup, notice),
            (Some(frame), None) | (None, Some(frame)) => frame,
            (None, None) => {
                crate::platform::hide_surface(&window)
                    .context("failed to hide empty notifications surface")?;
                return Ok(());
            }
        };
        super::presentation::stage_transient_update(&window).await?;
        // Captured after our own stage: only a competing content update staged
        // later may cancel the reveal at the end of this placement.
        let token = crate::platform::transient_presentation_token(&window)?;
        crate::platform::window::place_surface(
            &window,
            SurfaceRole::Notifications,
            PhysicalPosition::new(frame.x, frame.y),
            PhysicalSize::new(frame.w, frame.h),
            PhysicalPosition::new(area.x, area.y),
            scale,
            true,
        )
        .await?;
        let local = |subject: ScreenRect| LocalPoint {
            x: f64::from(subject.x - frame.x) / scale,
            y: f64::from(subject.y - frame.y) / scale,
        };
        window
            .emit(
                "notification-placement",
                NotificationPlacement {
                    popup: popup.map(local),
                    notice: notice.map(local),
                },
            )
            .context("failed to send notification placement")?;
        crate::platform::present_transient(&window, token)
            .context("failed to present notifications")?;
        Ok(())
    }
}

fn popup_anchors(position: PopupPosition) -> (HorizontalAnchor, VerticalAnchor) {
    match position {
        PopupPosition::TopLeft => (HorizontalAnchor::Left, VerticalAnchor::Top),
        PopupPosition::TopCenter => (HorizontalAnchor::Center, VerticalAnchor::Top),
        PopupPosition::TopRight => (HorizontalAnchor::Right, VerticalAnchor::Top),
        PopupPosition::BottomLeft => (HorizontalAnchor::Left, VerticalAnchor::Bottom),
        PopupPosition::BottomCenter => (HorizontalAnchor::Center, VerticalAnchor::Bottom),
        PopupPosition::BottomRight => (HorizontalAnchor::Right, VerticalAnchor::Bottom),
    }
}

fn bar_on_edge(
    bar: Option<ScreenRect>,
    position: BarPosition,
    edge: VerticalAnchor,
) -> Option<ScreenRect> {
    match (position, edge) {
        (BarPosition::Top, VerticalAnchor::Top) | (BarPosition::Bottom, VerticalAnchor::Bottom) => {
            bar
        }
        _ => None,
    }
}

#[tauri::command]
pub async fn show_notice(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
    key: String,
    ttl_ms: Option<u32>,
) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("notice key must be 1-128 ASCII key characters".to_string());
    }
    manager
        .show_notice(
            &app,
            NoticeRequest {
                key,
                ttl_ms: ttl_ms.unwrap_or(3_000).clamp(250, 60_000),
            },
        )
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn set_notification_measure(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, SurfaceManager>,
    measure: NotificationMeasure,
) -> Result<(), String> {
    manager
        .set_notification_measure(&app, measure, &state.config())
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn stage_notification_update(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != SurfaceRole::Notifications.label() {
        return Err(
            "notification updates can only originate from the notifications surface".into(),
        );
    }
    super::presentation::stage_transient_update(&window)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use smabar_core::config::PopupPosition;
    use smabar_core::platform::surfaces::{
        HorizontalAnchor, ScreenRect, VerticalAnchor, anchored_frame,
    };

    use super::popup_anchors;

    #[test]
    fn every_popup_position_maps_to_its_screen_edge() {
        for (position, expected) in [
            (
                PopupPosition::TopLeft,
                (HorizontalAnchor::Left, VerticalAnchor::Top),
            ),
            (
                PopupPosition::TopCenter,
                (HorizontalAnchor::Center, VerticalAnchor::Top),
            ),
            (
                PopupPosition::TopRight,
                (HorizontalAnchor::Right, VerticalAnchor::Top),
            ),
            (
                PopupPosition::BottomLeft,
                (HorizontalAnchor::Left, VerticalAnchor::Bottom),
            ),
            (
                PopupPosition::BottomCenter,
                (HorizontalAnchor::Center, VerticalAnchor::Bottom),
            ),
            (
                PopupPosition::BottomRight,
                (HorizontalAnchor::Right, VerticalAnchor::Bottom),
            ),
        ] {
            assert_eq!(popup_anchors(position), expected);
        }
    }

    #[test]
    fn corner_popups_only_dodge_a_bar_they_overlap() {
        let area = ScreenRect {
            x: 0,
            y: 0,
            w: 1_000,
            h: 800,
        };
        let centered_bar = ScreenRect {
            x: 350,
            y: 740,
            w: 300,
            h: 40,
        };
        let popup = |bar| {
            anchored_frame(
                area,
                (300, 100),
                HorizontalAnchor::Right,
                VerticalAnchor::Bottom,
                20,
                Some(bar),
                12,
            )
        };

        assert_eq!(popup(centered_bar).y, 680);
        assert_eq!(
            popup(ScreenRect {
                x: 600,
                ..centered_bar
            })
            .y,
            628
        );
    }
}
