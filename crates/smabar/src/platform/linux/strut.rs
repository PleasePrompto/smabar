//! Reserve screen space for the bar so other windows (and desktop icons) are
//! laid out NEXT TO it instead of under it.
//!
//! `_NET_WM_STRUT_PARTIAL` is a property on our window in ROOT screen
//! coordinates; the WM removes it automatically when the window goes away.
//! Wayland compositors get the equivalent layer-shell exclusive zone.

use std::os::raw::c_ulong;

use anyhow::Context;
use gtk::gdk;
use gtk::gdk::prelude::MonitorExt;
use gtk::prelude::WidgetExt;
use smabar_core::platform::{Rect, SessionKind, WindowBackend};
use tauri::{PhysicalPosition, WebviewWindow};

/// Docked edge the bar occupies; `None` clears the reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEdge {
    Top,
    Bottom,
}

fn exclusive_zone(dock: Option<(DockEdge, Rect)>, surface_height: i32) -> i32 {
    let zone = match dock {
        Some((DockEdge::Top, bar)) => i64::from(bar.y) + i64::from(bar.h),
        Some((DockEdge::Bottom, bar)) => i64::from(surface_height) - i64::from(bar.y),
        None => 0,
    };
    i32::try_from(zone.max(0)).unwrap_or(i32::MAX)
}

/// 12 cardinals: left, right, top, bottom, then the per-edge start/end pairs.
type StrutPartial = [c_ulong; 12];

fn strut_values(
    edge: DockEdge,
    bar: Rect,
    win_x: i32,
    win_y: i32,
    scale: f64,
    screen_h: i32,
) -> StrutPartial {
    let px = |v: i32| -> c_ulong { c_ulong::try_from(v.max(0)).unwrap_or(0) };
    let scaled = |v: i32| -> i32 { (f64::from(v) * scale).round() as i32 };
    let mut strut: StrutPartial = [0; 12];
    let bar_x = win_x + scaled(bar.x);
    let bar_y = win_y + scaled(bar.y);
    let bar_w = scaled(i32::try_from(bar.w).unwrap_or(i32::MAX));
    let bar_h = scaled(i32::try_from(bar.h).unwrap_or(i32::MAX));
    match edge {
        DockEdge::Top => {
            strut[2] = px(bar_y + bar_h); // thickness from the SCREEN top
            strut[8] = px(bar_x);
            strut[9] = px(bar_x + bar_w - 1);
        }
        DockEdge::Bottom => {
            // Thickness counts from the screen's BOTTOM edge.
            strut[3] = px(screen_h - bar_y);
            strut[10] = px(bar_x);
            strut[11] = px(bar_x + bar_w - 1);
        }
    }
    strut
}

/// True when the strut strip for `edge` would overlap a monitor other than
/// the bar's own — the EWMH strut coordinate space cannot express "reserve on
/// this monitor only", so such docks must not reserve space at all.
fn strip_hits_foreign_monitor(
    gdk_window: &gdk::Window,
    edge: DockEdge,
    bar: Rect,
    win_x: i32,
    win_y: i32,
    scale: f64,
) -> bool {
    let scaled = |v: i32| -> i32 { (f64::from(v) * scale).round() as i32 };
    let bar_x = win_x + scaled(bar.x);
    let bar_y = win_y + scaled(bar.y);
    let bar_w = scaled(i32::try_from(bar.w).unwrap_or(i32::MAX));
    let bar_h = scaled(i32::try_from(bar.h).unwrap_or(i32::MAX));
    // The reserved strip in root coordinates: from the screen edge up to the
    // bar's far side, limited to the bar's extent along the edge.
    let (x1, y1, x2, y2) = match edge {
        DockEdge::Top => (bar_x, 0, bar_x + bar_w, bar_y + bar_h),
        DockEdge::Bottom => (bar_x, bar_y, bar_x + bar_w, i32::MAX),
    };

    let display = gdk_window.display();
    let own = display.monitor_at_window(gdk_window);
    (0..display.n_monitors())
        .filter_map(|i| display.monitor(i))
        .filter(|monitor| own.as_ref() != Some(monitor))
        .any(|monitor| {
            let g = monitor.geometry();
            let sf = monitor.scale_factor();
            let (mx1, my1) = (g.x() * sf, g.y() * sf);
            let (mx2, my2) = (mx1 + g.width() * sf, my1 + g.height() * sf);
            x1 < mx2 && mx1 < x2 && y1 < my2 && my1 < y2
        })
}

/// Applies (or clears, with `None`) the platform reservation for the bar rect
/// given in logical window coordinates.
pub fn apply(
    window: &WebviewWindow,
    dock: Option<(DockEdge, Rect)>,
    target_origin: Option<PhysicalPosition<i32>>,
) -> anyhow::Result<()> {
    match super::window::backend() {
        WindowBackend::WaylandLayerShell => return apply_layer_shell(window, dock),
        WindowBackend::WaylandFallback => return Ok(()),
        WindowBackend::X11 | WindowBackend::Other => {}
    }

    let session =
        SessionKind::from_xdg_session_type(std::env::var("XDG_SESSION_TYPE").ok().as_deref());
    if session != SessionKind::X11 {
        tracing::warn!(?session, "struts are only implemented for X11");
        return Ok(());
    }
    let win = window.clone();
    window
        .run_on_main_thread(move || {
            let Ok(gtk_window) = win.gtk_window() else {
                tracing::error!("failed to resolve GTK window for strut");
                return;
            };
            let Some(gdk_window) = gtk_window.window() else {
                tracing::error!("bar window is not realized; cannot set strut");
                return;
            };
            let scale = win.scale_factor().unwrap_or(1.0);
            let (win_x, win_y) = match target_origin {
                Some(pos) => (pos.x, pos.y),
                None => match win.outer_position() {
                    Ok(pos) => (pos.x, pos.y),
                    Err(error) => {
                        tracing::error!(%error, "failed to read window position for strut");
                        return;
                    }
                },
            };
            // Root window height == total X screen height in pixels.
            let screen_h = gdk_window
                .screen()
                .root_window()
                .map_or(0, |root| root.height());
            let strut: StrutPartial = match dock {
                Some((edge, bar)) => {
                    // EWMH struts measure from the SCREEN edge. On an inner
                    // monitor edge the reserved strip would cover every other
                    // monitor between the bar and that screen edge — refuse
                    // instead of wrecking the neighbour monitor's layout.
                    if strip_hits_foreign_monitor(&gdk_window, edge, bar, win_x, win_y, scale) {
                        tracing::warn!(
                            ?edge,
                            "docked edge is not a screen edge on this monitor; \
                             not reserving space (windows may overlap the bar)"
                        );
                        [0; 12]
                    } else {
                        strut_values(edge, bar, win_x, win_y, scale, screen_h)
                    }
                }
                None => [0; 12],
            };
            let cardinal = gdk::Atom::intern("CARDINAL");
            gdk::property_change(
                &gdk_window,
                &gdk::Atom::intern("_NET_WM_STRUT_PARTIAL"),
                &cardinal,
                32,
                gdk::PropMode::Replace,
                gdk::ChangeData::ULongs(&strut),
            );
            // Legacy fallback for WMs that only read the 4-value strut.
            gdk::property_change(
                &gdk_window,
                &gdk::Atom::intern("_NET_WM_STRUT"),
                &cardinal,
                32,
                gdk::PropMode::Replace,
                gdk::ChangeData::ULongs(&strut[0..4]),
            );
            tracing::debug!(?dock, ?strut, "strut applied");
        })
        .context("failed to schedule strut update on the main thread")
}

fn apply_layer_shell(window: &WebviewWindow, dock: Option<(DockEdge, Rect)>) -> anyhow::Result<()> {
    let win = window.clone();
    window
        .run_on_main_thread(move || {
            let Ok(gtk_window) = win.gtk_window() else {
                tracing::error!("failed to resolve GTK window for exclusive-zone update");
                return;
            };
            let surface_height = gtk_window.allocated_height();
            let requested_zone = exclusive_zone(dock, surface_height);
            if let Err(error) =
                super::reservation::apply(&gtk_window, dock.map(|(edge, _)| edge), requested_zone)
            {
                tracing::error!(%error, "failed to update the Wayland reservation");
                return;
            }
            tracing::debug!(
                ?dock,
                exclusive_zone = requested_zone,
                "exclusive zone applied"
            );
        })
        .context("failed to schedule exclusive-zone update on the main thread")
}

#[cfg(test)]
mod tests {
    use super::{DockEdge, exclusive_zone, strut_values};
    use smabar_core::platform::Rect;

    #[test]
    fn derives_wayland_exclusive_zone_from_logical_bar_geometry() {
        let rect = |y, h| Rect {
            x: 120,
            y,
            w: 800,
            h,
        };

        assert_eq!(
            exclusive_zone(Some((DockEdge::Top, rect(8, 40))), 1_060),
            48
        );
        assert_eq!(
            exclusive_zone(Some((DockEdge::Bottom, rect(1_012, 40))), 1_060),
            48
        );
        assert_eq!(exclusive_zone(None, 1_060), 0);
        assert_eq!(
            exclusive_zone(Some((DockEdge::Top, rect(-20, 4))), 1_060),
            0
        );
        assert_eq!(
            exclusive_zone(Some((DockEdge::Top, rect(i32::MAX, u32::MAX))), 1_060),
            i32::MAX
        );
    }

    #[test]
    fn edge_change_strut_uses_the_target_window_origin() {
        let bar = Rect {
            x: 0,
            y: 22,
            w: 2_560,
            h: 46,
        };
        let strut = strut_values(DockEdge::Bottom, bar, 0, 2_984, 1.0, 3_152);

        assert_eq!(strut[3], 146);
        assert_eq!(strut[10], 0);
        assert_eq!(strut[11], 2_559);
    }
}
