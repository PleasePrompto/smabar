//! Windows AppBar work-area reservation.

use anyhow::Context;
use smabar_core::platform::Rect;
use tauri::{PhysicalPosition, WebviewWindow};

pub use crate::platform::windows_geometry::DockEdge;

pub fn apply(
    window: &WebviewWindow,
    dock: Option<(DockEdge, Rect)>,
    target_origin: Option<PhysicalPosition<i32>>,
) -> anyhow::Result<()> {
    let bar = window.clone();
    window
        .run_on_main_thread(move || {
            let hwnd = match super::native::window_hwnd(&bar) {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    tracing::error!(%error, ?dock, "failed to update the Windows AppBar reservation");
                    return;
                }
            };
            if let Some(origin) = target_origin {
                super::placement::record(origin.x, origin.y);
            }
            if let Err(error) = super::native::set_reservation(hwnd, dock) {
                tracing::error!(
                    %error,
                    ?dock,
                    "failed to update the Windows AppBar reservation; maximized windows may overlap the bar"
                );
            }
            // Shrinking the work area makes the shell push the host out of the
            // reserved strip (see `placement`). Most of those pushes arrive
            // asynchronously and are undone from WM_WINDOWPOSCHANGED; this
            // catches one that already happened inside the shell call.
            super::placement::reconcile(hwnd);
        })
        .context("failed to schedule the Windows AppBar update on the main thread")
}
