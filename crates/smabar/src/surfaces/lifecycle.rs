//! The one mutable state record behind `SurfaceManager`'s mutex.

use std::collections::HashSet;

use serde_json::Value;
use smabar_core::config::BarPosition;
use smabar_core::platform::Rect;
use smabar_core::platform::surfaces::ScreenRect;

use super::SurfaceRole;
use super::model::{
    ActiveFlyout, ActiveMenu, ActiveTooltip, FlyoutLayout, NoticeRequest, NotificationMeasure,
    TooltipLayout,
};

#[derive(Default)]
pub(super) struct Lifecycle {
    pub(super) displays: Vec<crate::platform::display::DisplaySnapshot>,
    pub(super) effective_monitor_id: Option<String>,
    pub(super) ready: HashSet<SurfaceRole>,
    pub(super) settings_group: Option<String>,
    pub(super) pending_popups: Vec<Value>,
    pub(super) pending_notice: Option<NoticeRequest>,
    pub(super) bar_work_area: Option<ScreenRect>,
    pub(super) bar_rect: Option<ScreenRect>,
    pub(super) bar_geometry: Option<(BarPosition, Rect)>,
    pub(super) bar_surface_frame: Option<ScreenRect>,
    pub(super) bar_relocation: Option<BarPosition>,
    pub(super) bar_relocation_frame: u64,
    pub(super) bar_revealed: bool,
    pub(super) bar_offset: u32,
    pub(super) bar_visible_height: u32,
    pub(super) bar_motion_generation: u64,
    /// Bumped per reservation request; a deferred application checks it.
    pub(super) bar_reservation_generation: u64,
    pub(super) notification_measure: Option<NotificationMeasure>,
    pub(super) overlay_generation: u64,
    /// `overlay_generation` at the overlay's last focus gain. Focus-loss
    /// dismissal only applies to content at least this old: the click that
    /// opens the next flyout also unfocuses the overlay, and that
    /// `Focused(false)` can arrive after the open — closing the newer
    /// content would eat the click.
    pub(super) overlay_focus_generation: u64,
    /// Whether the overlay window holds keyboard focus right now. A focus
    /// loss is acted on only after a short grace, and only while this is
    /// still false — the click that switches flyouts unfocuses the overlay
    /// a few ms before its open arrives, and the replacement regains focus.
    pub(super) overlay_focused: bool,
    pub(super) active_flyout: Option<ActiveFlyout>,
    pub(super) flyout_layout: Option<FlyoutLayout>,
    pub(super) active_menu: Option<ActiveMenu>,
    pub(super) menu_frame: Option<ScreenRect>,
    pub(super) active_tooltip: Option<ActiveTooltip>,
    pub(super) tooltip_layout: Option<TooltipLayout>,
    pub(super) overlay_frame: Option<ScreenRect>,
}

impl Lifecycle {
    pub(super) fn trigger_frame(&self, role: SurfaceRole) -> Option<ScreenRect> {
        match role {
            // Opening an interaction reveals autohide. Anchor to its settled
            // frame even when the request arrives during native movement.
            SurfaceRole::Bar => self.bar_surface_frame,
            SurfaceRole::Overlay => self.overlay_frame,
            SurfaceRole::Settings | SurfaceRole::Notifications => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_origin_follows_edges_and_the_revealed_bar_when_autohide_opens_a_flyout() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 60,
        };
        let bottom = ScreenRect {
            x: 240,
            y: 740,
            w: 800,
            h: 60,
        };
        let mut lifecycle = Lifecycle {
            bar_geometry: Some((BarPosition::Bottom, rect)),
            bar_surface_frame: Some(bottom),
            ..Lifecycle::default()
        };
        assert_eq!(lifecycle.trigger_frame(SurfaceRole::Bar), Some(bottom));
        lifecycle.bar_geometry = Some((BarPosition::Top, rect));
        lifecycle.bar_surface_frame = Some(ScreenRect { y: 26, ..bottom });
        assert_eq!(
            lifecycle
                .trigger_frame(SurfaceRole::Bar)
                .map(|frame| frame.y),
            Some(26)
        );
        lifecycle.bar_geometry = Some((BarPosition::Bottom, rect));
        lifecycle.bar_surface_frame = Some(bottom);
        lifecycle.bar_offset = 52;
        assert_eq!(
            lifecycle
                .trigger_frame(SurfaceRole::Bar)
                .map(|frame| frame.y),
            Some(740)
        );
    }
}
