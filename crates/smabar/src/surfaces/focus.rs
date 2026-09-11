//! Overlay keyboard-focus tracking and the focus-loss dismissal of transient
//! content (pinned flyouts, menus, the overlay row).

use std::time::Duration;

use anyhow::Context;
use tauri::{AppHandle, Emitter, Manager};

use super::lifecycle::Lifecycle;
use super::model::{FlyoutMode, OverlayFlyoutRequest};
use super::{SurfaceManager, SurfaceRole};

/// How long a focus loss may be contradicted before it dismisses the overlay.
/// Switching flyouts unfocuses the overlay first and opens the replacement a
/// few ms later; without the grace the old content ran its full close chain
/// (conceal, paint, unmap) in parallel with the replacement's open.
const FOCUS_LOSS_GRACE: Duration = Duration::from_millis(50);

impl SurfaceManager {
    pub fn overlay_generation(&self) -> anyhow::Result<u64> {
        self.lifecycle
            .lock()
            .map(|state| state.overlay_generation)
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))
    }

    pub fn handle_focus_gained(&self) -> anyhow::Result<()> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        lifecycle.overlay_focus_generation = lifecycle.overlay_generation;
        lifecycle.overlay_focused = true;
        tracing::debug!(
            generation = lifecycle.overlay_generation,
            "overlay gained focus"
        );
        Ok(())
    }

    pub fn handle_focus_lost(&self, app: &AppHandle) -> anyhow::Result<()> {
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .overlay_focused = false;
        tracing::debug!("overlay lost focus; grace started");
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FOCUS_LOSS_GRACE).await;
            if let Err(error) = app.state::<SurfaceManager>().dismiss_overlay(&app, None) {
                tracing::error!(
                    %error,
                    "failed to dismiss the overlay after focus moved away; press Escape to close it"
                );
            }
        });
        Ok(())
    }

    /// A compositor grab supplies its generation; ordinary focus loss uses
    /// the most recent focus gain and honors the focus-restoration grace.
    pub fn dismiss_overlay(&self, app: &AppHandle, grabbed_at: Option<u64>) -> anyhow::Result<()> {
        let (menu_closed, flyout_closed) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            let Some(dismissed) = take_dismissed_overlay(&mut lifecycle, grabbed_at) else {
                return Ok(());
            };
            dismissed
        };
        self.report_bar_pointer(app)?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = async {
                app.state::<SurfaceManager>().present_overlay(&app).await?;
                if let Some(overlay) = app.get_webview_window(SurfaceRole::Overlay.label()) {
                    if menu_closed {
                        overlay
                            .emit("menu-closed", ())
                            .context("failed to clear context menu after focus loss")?;
                    }
                    if let Some(closed) = flyout_closed.as_ref() {
                        overlay
                            .emit("flyout-closed", closed)
                            .context("failed to clear flyout after focus loss")?;
                    }
                }
                if let Some(closed) = flyout_closed
                    && let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label())
                {
                    bar.emit("flyout-closed", closed)
                        .context("failed to report focus-dismissed flyout to bar")?;
                }
                anyhow::Ok(())
            }
            .await;
            if let Err(error) = result {
                tracing::error!(
                    %error,
                    "failed to dismiss the overlay after focus moved away; press Escape to close it"
                );
            }
        });
        Ok(())
    }
}

pub(super) fn dismisses_on_focus_loss(menu_open: bool, flyout_mode: Option<FlyoutMode>) -> bool {
    menu_open || flyout_mode == Some(FlyoutMode::Pinned)
}

/// Whether a `Focused(false)` belongs to a previous overlay state: the newest
/// open content was created after the overlay last gained focus, so this
/// focus loss predates it and must not dismiss it.
pub(super) fn focus_loss_is_stale(flyout: Option<u64>, menu: Option<u64>, focused_at: u64) -> bool {
    [flyout, menu]
        .into_iter()
        .flatten()
        .max()
        .is_some_and(|newest| newest > focused_at)
}

fn take_dismissed_overlay(
    lifecycle: &mut Lifecycle,
    grabbed_at: Option<u64>,
) -> Option<(bool, Option<OverlayFlyoutRequest>)> {
    if grabbed_at.is_none() && lifecycle.overlay_focused {
        tracing::debug!("overlay regained focus within the grace; keeping it");
        return None;
    }
    let flyout_mode = lifecycle
        .active_flyout
        .as_ref()
        .map(|active| active.request.mode);
    if !dismisses_on_focus_loss(lifecycle.active_menu.is_some(), flyout_mode) {
        return None;
    }
    // A click on the next trigger races its open against the overlay's
    // `Focused(false)`. Content newer than the overlay's last focus
    // gain was opened after the real focus loss; dismissing it would
    // eat that click (the flyout would only open on the second one).
    let newest = [
        lifecycle
            .active_flyout
            .as_ref()
            .map(|active| active.request.generation),
        lifecycle
            .active_menu
            .as_ref()
            .map(|active| active.request.generation),
    ];
    let focused_at = grabbed_at.unwrap_or(lifecycle.overlay_focus_generation);
    if focus_loss_is_stale(newest[0], newest[1], focused_at) {
        tracing::debug!(?newest, focused_at, "ignored stale overlay focus loss");
        return None;
    }
    tracing::debug!(
        ?newest,
        focused_at,
        compositor_grab = grabbed_at.is_some(),
        "dismissing overlay after outside interaction"
    );
    let menu_closed = lifecycle.active_menu.take().is_some();
    lifecycle.menu_frame = None;
    let flyout_closed = lifecycle.active_flyout.take().map(|active| active.request);
    lifecycle.flyout_layout = None;
    lifecycle.overlay_generation = lifecycle.overlay_generation.wrapping_add(1);
    Some((menu_closed, flyout_closed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::model::ActiveFlyout;
    use smabar_core::platform::surfaces::ScreenRect;

    #[test]
    fn native_dismissal_works_without_focus_loss_and_rejects_stale_grabs() {
        let mut lifecycle = Lifecycle {
            overlay_generation: 5,
            overlay_focus_generation: 5,
            overlay_focused: true,
            active_flyout: Some(ActiveFlyout {
                request: OverlayFlyoutRequest {
                    generation: 5,
                    tile_id: "plugin:clock:clock".into(),
                    mode: FlyoutMode::Pinned,
                    preserve_content: false,
                },
                trigger: ScreenRect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 40,
                },
            }),
            ..Lifecycle::default()
        };
        // Restored keyboard focus cancels the ordinary grace timer.
        assert!(take_dismissed_overlay(&mut lifecycle, None).is_none());
        // A delayed event from the previous tile must not close this one.
        assert!(take_dismissed_overlay(&mut lifecycle, Some(4)).is_none());
        assert!(lifecycle.active_flyout.is_some());
        let (_, closed) = take_dismissed_overlay(&mut lifecycle, Some(5)).unwrap();
        assert_eq!(closed.unwrap().tile_id, "plugin:clock:clock");
        assert!(lifecycle.active_flyout.is_none());
        assert_eq!(lifecycle.overlay_generation, 6);
    }
}
