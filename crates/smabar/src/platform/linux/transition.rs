//! Keeps mapped transparent WebKit surfaces invisible during native changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use gtk::glib::CastNone;
use gtk::prelude::{GtkWindowExt, ObjectExt, WidgetExt};
use smabar_core::platform::WindowBackend;
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

use crate::surfaces::SurfaceRole;

static OVERLAY_PAINT_GENERATION: AtomicU64 = AtomicU64::new(0);
static NOTIFICATION_PAINT_GENERATION: AtomicU64 = AtomicU64::new(0);
const NATIVE_PAINTS: u8 = 2;

pub async fn set_opaque(window: &WebviewWindow, opaque: bool) -> anyhow::Result<bool> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let bar = window.clone();
    window
        .run_on_main_thread(move || {
            let result = bar
                .gtk_window()
                .map(|gtk_window| gtk_window.set_opacity(if opaque { 1.0 } else { 0.0 }))
                .map_err(|error| format!("{error:#}"));
            let _ = send.send(result);
        })
        .context("failed to schedule bar opacity during edge transition")?;
    receive
        .await
        .context("bar opacity callback was dropped")?
        .map_err(anyhow::Error::msg)?;
    Ok(true)
}

/// Conceals a transient before replacement. X11 keeps the accelerated buffer
/// mapped; Wayland withdraws the surface until its new configure and paint.
pub async fn stage_transient_update(window: &WebviewWindow) -> anyhow::Result<()> {
    let generation = paint_generation(window)?;
    generation.fetch_add(1, Ordering::Relaxed);
    let (send, receive) = tokio::sync::oneshot::channel();
    let staged = window.clone();
    window
        .with_webview(move |platform| {
            let webview = platform.inner().clone();
            let Some(gtk_window) = webview.toplevel().and_downcast::<gtk::ApplicationWindow>()
            else {
                let _ = send.send(Err(format!(
                    "WebKit transient `{}` has no GTK top-level",
                    staged.label()
                )));
                return;
            };
            match set_native_opacity(&gtk_window, 0.0) {
                Ok(()) if buffer_opacity() => {
                    // A fully transparent Wayland buffer may receive no frame
                    // callback. Unmapping also prevents the old buffer from
                    // flashing at the replacement's new layer-shell position.
                    gtk_window.hide();
                    let _ = send.send(Ok(()));
                }
                Ok(()) if gtk_window.is_mapped() => {
                    complete_conceal_after_native_paint(&gtk_window, &webview, send);
                }
                Ok(()) => {
                    let _ = send.send(Ok(()));
                }
                Err(error) => {
                    let _ = send.send(Err(format!("{error:#}")));
                }
            }
        })
        .context("failed to schedule transient surface concealment")?;
    receive
        .await
        .context("transient surface concealment callback was dropped")?
        .map_err(anyhow::Error::msg)
}

/// Applies transient geometry on GTK's window thread before reveal can begin.
pub async fn place_surface(
    window: &WebviewWindow,
    role: SurfaceRole,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    monitor_origin: PhysicalPosition<i32>,
    scale: f64,
    keyboard: bool,
) -> anyhow::Result<()> {
    if role == SurfaceRole::Bar || role == SurfaceRole::Settings {
        anyhow::bail!("`{}` is not a transient positioned surface", role.label());
    }
    let width = (f64::from(size.width) / scale).round().max(1.0) as i32;
    let height = (f64::from(size.height) / scale).round().max(1.0) as i32;
    let wayland = super::window::backend() == WindowBackend::WaylandLayerShell;
    let (x, y) = transient_coordinates(position, monitor_origin, scale, wayland);
    let (send, receive) = tokio::sync::oneshot::channel();
    let positioned = window.clone();
    window
        .with_webview(move |platform| {
            let result = positioned
                .gtk_window()
                .map_err(|error| format!("{error:#}"))
                .and_then(|gtk_window| {
                    if wayland {
                        super::layer::place_transient(&positioned, &gtk_window, x, y, height, keyboard)
                            .map_err(|error| format!("{error:#}"))?;
                    }
                    // X11 needs an expose barrier across separate configure
                    // events; Wayland commits buffer size and contents together.
                    let frozen = gtk_window.window().filter(|surface| !buffer_opacity() && surface.is_visible() && (surface.width() != width || surface.height() != height));
                    if let Some(surface) = &frozen { surface.freeze_updates(); }
                    gtk_window.set_resizable(true);
                    gtk_window.set_size_request(width, height);
                    gtk_window.resize(width, height);
                    if !wayland {
                        // Muffin/Mutter apply a move only to a mapped window:
                        // a position handed to a withdrawn one is dropped at
                        // its next map, which re-opened a short flyout at the
                        // previous, taller frame's y. Staging already set the
                        // surface to opacity 0, so mapping here shows nothing;
                        // the reveal still waits for the WebKit paint.
                        if !gtk_window.is_mapped() {
                            gtk_window.show();
                        }
                        // GTK defers both the size request's WM hints and the
                        // resize to its idle, so a plain `move_` reached the
                        // window manager first — while the window still had
                        // its old size AND the old minimum-size hint. Muffin
                        // refused the shrink below that minimum and clamped
                        // the move so the old, taller window stayed on the
                        // work area; the later shrink landed at the clamped y
                        // (a flyout's row sat a screen-height too high after
                        // the flyout closed). So the hint goes out
                        // synchronously and size and position travel in one
                        // configure request.
                        match gtk_window.window() {
                            Some(gdk_window) => {
                                gdk_window.set_geometry_hints(
                                    &gtk::gdk::Geometry::new(
                                        width,
                                        height,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0.0,
                                        0.0,
                                        gtk::gdk::Gravity::NorthWest,
                                    ),
                                    gtk::gdk::WindowHints::MIN_SIZE,
                                );
                                gdk_window.move_resize(x, y, width, height);
                            }
                            None => gtk_window.move_(x, y),
                        }
                    }
                    gtk_window.display().sync();
                    Ok(frozen)
                });
            match result {
                Ok(Some(surface)) => platform.inner().snapshot(
                    SnapshotRegion::Visible,
                    SnapshotOptions::TRANSPARENT_BACKGROUND,
                    None::<&gtk::gio::Cancellable>,
                    move |result| {
                        if let Err(error) = result {
                            tracing::warn!(%error, "resize render barrier failed; native redraw resumed");
                        }
                        thaw_after_resize(&surface, send);
                    },
                ),
                result => { let _ = send.send(result.map(|_| ())); }
            }
        })
        .with_context(|| format!("failed to schedule `{}` placement", role.label()))?;
    receive
        .await
        .with_context(|| format!("`{}` placement callback was dropped", role.label()))?
        .map_err(anyhow::Error::msg)
}

/// The snapshot reply precedes the accelerated buffer swap. Keep exposes
/// frozen for the same native frame barrier used by ordinary presentation.
fn thaw_after_resize(
    surface: &gtk::gdk::Window,
    send: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let Some(clock) = surface.frame_clock() else {
        surface.thaw_updates();
        let _ = send.send(Ok(()));
        return;
    };
    let remaining = Cell::new(NATIVE_PAINTS);
    let handler = Rc::new(RefCell::new(None));
    let callback_handler = Rc::clone(&handler);
    let pending_send = RefCell::new(Some(send));
    let surface = surface.clone();
    *handler.borrow_mut() = Some(clock.connect_after_paint(move |clock| {
        if remaining.get() > 1 {
            remaining.set(remaining.get() - 1);
            clock.request_phase(gtk::gdk::FrameClockPhase::AFTER_PAINT);
            return;
        }
        if let Some(handler) = callback_handler.borrow_mut().take() {
            clock.disconnect(handler);
        }
        surface.thaw_updates();
        if let Some(send) = pending_send.borrow_mut().take() {
            let _ = send.send(Ok(()));
        }
    }));
    clock.request_phase(gtk::gdk::FrameClockPhase::AFTER_PAINT);
}

fn transient_coordinates(
    position: PhysicalPosition<i32>,
    monitor_origin: PhysicalPosition<i32>,
    scale: f64,
    wayland: bool,
) -> (i32, i32) {
    let origin = if wayland {
        monitor_origin
    } else {
        PhysicalPosition::new(0, 0)
    };
    let x = (f64::from(position.x - origin.x) / scale).round() as i32;
    let y = (f64::from(position.y - origin.y) / scale).round() as i32;
    if wayland {
        (x.max(0), y.max(0))
    } else {
        (x, y)
    }
}

fn complete_conceal_after_native_paint(
    window: &gtk::ApplicationWindow,
    webview: &webkit2gtk::WebView,
    send: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let Some(clock) = window.frame_clock() else {
        let _ = send.send(Err("mapped GTK transient has no frame clock".to_string()));
        return;
    };
    let remaining = Cell::new(NATIVE_PAINTS);
    let handler = Rc::new(RefCell::new(None));
    let callback_handler = Rc::clone(&handler);
    let pending_send = Rc::new(RefCell::new(Some(send)));
    let callback_send = Rc::clone(&pending_send);
    let weak_window = window.downgrade();
    let weak_webview = webview.downgrade();
    let handler_id = clock.connect_after_paint(move |clock| {
        if remaining.get() > 1 {
            remaining.set(remaining.get() - 1);
            if let Some(webview) = weak_webview.upgrade() {
                webview.queue_draw();
            }
            return;
        }
        if let Some(handler_id) = callback_handler.borrow_mut().take() {
            clock.disconnect(handler_id);
        }
        let result = weak_window
            .upgrade()
            .ok_or_else(|| "GTK transient disappeared during concealment".to_string())
            .map(|window| window.display().sync());
        if let Some(send) = callback_send.borrow_mut().take() {
            let _ = send.send(result);
        }
    });
    *handler.borrow_mut() = Some(handler_id);
    webview.queue_draw();
}

/// The freshness proof a reveal or hide must carry. Capture it right after
/// the stage that produced the content being presented; every later stage
/// invalidates it, so a reveal scheduled for superseded content can never
/// expose the previous buffer.
pub fn presentation_token(window: &WebviewWindow) -> anyhow::Result<u64> {
    Ok(paint_generation(window)?.load(Ordering::Relaxed))
}

/// Forces WebKitGTK to render the replacement before revealing its native
/// window. GTK frame signals alone can precede the accelerated WebKit buffer.
pub fn reveal_transient_after_paint(window: &WebviewWindow, expected: u64) -> anyhow::Result<()> {
    finish_transient_after_paint(window, expected, true)
}

/// Lets WebKitGTK commit the empty replacement buffer before unmapping the
/// transient, so its next show cannot resurrect the previous contents.
pub fn hide_transient_after_paint(window: &WebviewWindow, expected: u64) -> anyhow::Result<()> {
    finish_transient_after_paint(window, expected, false)
}

fn finish_transient_after_paint(
    window: &WebviewWindow,
    expected: u64,
    reveal: bool,
) -> anyhow::Result<()> {
    let generation = paint_generation(window)?;
    let transient = window.clone();
    window
        .with_webview(move |platform| {
            let webview = platform.inner().clone();
            let Some(gtk_window) = webview.toplevel().and_downcast::<gtk::ApplicationWindow>()
            else {
                tracing::error!(
                    surface = transient.label(),
                    "WebKit transient has no GTK top-level; paint completion cannot be finalized"
                );
                return;
            };
            let weak_window = gtk_window.downgrade();
            let weak_webview = webview.downgrade();
            webview.snapshot(
                SnapshotRegion::Visible,
                SnapshotOptions::TRANSPARENT_BACKGROUND,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if generation.load(Ordering::Relaxed) != expected {
                        return;
                    }
                    if let Err(error) = result {
                        tracing::warn!(
                            %error,
                            surface = transient.label(),
                            "WebKit render barrier failed; falling back to native paint completion"
                        );
                    }
                    let (Some(window), Some(webview)) =
                        (weak_window.upgrade(), weak_webview.upgrade())
                    else {
                        return;
                    };
                    finish_after_native_paint(
                        &window,
                        &webview,
                        generation,
                        expected,
                        transient.label().to_owned(),
                        reveal,
                    );
                },
            );
        })
        .context("failed to schedule the WebKit transient render barrier")
}

fn finish_after_native_paint(
    window: &gtk::ApplicationWindow,
    webview: &webkit2gtk::WebView,
    generation: &'static AtomicU64,
    expected: u64,
    label: String,
    reveal: bool,
) {
    if buffer_opacity() {
        // Wayland paints opacity into the buffer. At opacity zero GTK skips
        // the child draw, so waiting for further paints can stall forever.
        // The completed WebKit snapshot owns freshness; the next GTK paint
        // presents that content and its restored alpha atomically.
        if reveal {
            if let Err(error) = set_native_opacity(window, 1.0) {
                tracing::error!(%error, surface = label, "failed to reveal Wayland transient after WebKit rendered it");
            }
        } else {
            window.hide();
        }
        return;
    }
    let Some(clock) = window.frame_clock() else {
        if reveal {
            if let Err(error) = set_native_opacity(window, 1.0) {
                tracing::error!(%error, surface = label, "failed to reveal transient surface without a GTK frame clock");
            }
        } else {
            window.hide();
        }
        return;
    };
    let remaining = Cell::new(NATIVE_PAINTS);
    let handler = Rc::new(RefCell::new(None));
    let callback_handler = Rc::clone(&handler);
    let weak_window = window.downgrade();
    let weak_webview = webview.downgrade();
    let handler_id = clock.connect_after_paint(move |clock| {
        if remaining.get() > 1 {
            remaining.set(remaining.get() - 1);
            if let Some(webview) = weak_webview.upgrade() {
                webview.queue_draw();
            }
            return;
        }
        if let Some(handler_id) = callback_handler.borrow_mut().take() {
            clock.disconnect(handler_id);
        }
        if generation.load(Ordering::Relaxed) != expected {
            return;
        }
        if let Some(window) = weak_window.upgrade() {
            if reveal {
                if let Err(error) = set_native_opacity(&window, 1.0) {
                    tracing::error!(
                        %error,
                        surface = label,
                        "failed to reveal transient surface after WebKit rendered it"
                    );
                }
            } else {
                window.hide();
            }
        }
    });
    *handler.borrow_mut() = Some(handler_id);
    webview.queue_draw();
}

fn set_native_opacity(window: &gtk::ApplicationWindow, opacity: f64) -> anyhow::Result<()> {
    if !window.is_realized() {
        window.realize();
    }
    let surface = window
        .window()
        .context("GTK transient surface is not realized")?;
    if buffer_opacity() {
        // GDK's Wayland window-opacity function is a no-op. GTK must paint
        // the alpha into the buffer before the transient can be concealed.
        window.set_opacity(opacity);
    } else {
        surface.set_opacity(opacity);
    }
    surface.display().sync();
    Ok(())
}

fn buffer_opacity() -> bool {
    matches!(
        super::window::backend(),
        WindowBackend::WaylandLayerShell | WindowBackend::WaylandFallback
    )
}

fn paint_generation(window: &WebviewWindow) -> anyhow::Result<&'static AtomicU64> {
    match window.label() {
        "overlay" => Ok(&OVERLAY_PAINT_GENERATION),
        "notifications" => Ok(&NOTIFICATION_PAINT_GENERATION),
        label => anyhow::bail!("`{label}` is not a transient paint surface"),
    }
}

#[cfg(test)]
mod tests {
    use super::transient_coordinates;
    use tauri::PhysicalPosition;

    #[test]
    fn transient_coordinates_preserve_x11_space_and_localize_wayland() {
        let origin = PhysicalPosition::new(2_560, 155);
        assert_eq!(
            transient_coordinates(PhysicalPosition::new(-100, 20), origin, 2.0, false),
            (-50, 10)
        );
        assert_eq!(
            transient_coordinates(PhysicalPosition::new(3_000, 301), origin, 2.0, true),
            (220, 73)
        );
        assert_eq!(
            transient_coordinates(PhysicalPosition::new(2_500, 100), origin, 1.0, true),
            (0, 0)
        );
    }
}
