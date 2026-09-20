//! Display-backend input shape: which parts of the transparent window accept clicks.
//! Everything outside the shaped region falls through to the desktop.

use std::sync::Once;

use anyhow::Context;
use gtk::cairo;
use gtk::prelude::{DeviceExt, SeatExt, WidgetExt};
use smabar_core::platform::{Rect, SessionKind};
use tauri::WebviewWindow;

/// Call on the GTK thread. GDK returns no owned window when another app
/// covers the pointer; the toplevel check also rejects our own other surfaces.
pub fn pointer_sample(window: &WebviewWindow) -> anyhow::Result<Option<[f64; 2]>> {
    let native = window.gtk_window()?;
    let Some(surface) = native.window() else {
        return Ok(None);
    };
    let pointer = native
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer());
    let Some(pointer) = pointer else {
        return Ok(None);
    };
    let (focused, _, _) = pointer.window_at_position_double();
    if focused.is_none_or(|focused| focused.toplevel() != surface) {
        return Ok(None);
    }
    let (_, x, y, _) = surface.device_position_double(&pointer);
    Ok(Some([x, y]))
}

/// Applies `rects` as the window's input shape. An empty list yields an
/// empty region: nothing is clickable and every click reaches the desktop.
///
/// Coordinates: GTK input regions are in logical (scale-independent) pixels
/// and the webview maps CSS pixels 1:1 onto logical pixels, so the rects are
/// passed through unscaled even when the monitor scale factor is not 1.
///
/// GTK maps this region to the X11 Shape extension or the Wayland
/// `wl_surface` input region, depending on the active display backend.
pub fn apply(window: &WebviewWindow, rects: Vec<Rect>) -> anyhow::Result<()> {
    let session =
        SessionKind::from_xdg_session_type(std::env::var("XDG_SESSION_TYPE").ok().as_deref());
    if !matches!(session, SessionKind::X11 | SessionKind::Wayland) {
        warn_unsupported_once(session);
        return Ok(());
    }

    // GTK objects are not Send — resolve the handle inside the main thread.
    let win = window.clone();
    window
        .run_on_main_thread(move || match win.gtk_window() {
            Ok(gtk_window) => {
                let rectangles: Vec<cairo::RectangleInt> = rects
                    .iter()
                    .map(|r| {
                        cairo::RectangleInt::new(
                            r.x,
                            r.y,
                            i32::try_from(r.w).unwrap_or(i32::MAX),
                            i32::try_from(r.h).unwrap_or(i32::MAX),
                        )
                    })
                    .collect();
                let region = cairo::Region::create_rectangles(&rectangles);
                gtk_window.input_shape_combine_region(Some(&region));
                if session == SessionKind::Wayland {
                    gtk_window.queue_draw();
                }
                tracing::debug!(rect_count = rects.len(), "input shape applied");
            }
            Err(error) => {
                tracing::error!(%error, "failed to resolve GTK window for input shape");
            }
        })
        .context("failed to schedule input-shape update on the main thread")
}

fn warn_unsupported_once(session: SessionKind) {
    static WARN_ONCE: Once = Once::new();
    WARN_ONCE.call_once(|| {
        tracing::warn!(
            ?session,
            "input shaping is unsupported on this display backend; clicks outside the bar will not pass through"
        );
    });
}

/// Tauri command: the shell reports the clickable regions in CSS pixels.
#[tauri::command]
pub fn set_input_shape(window: WebviewWindow, rects: Vec<Rect>) -> Result<(), String> {
    apply(&window, rects).map_err(|error| format!("{error:#}"))
}
