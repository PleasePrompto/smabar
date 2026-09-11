//! Assigns existing GTK surfaces to the selected output.

use anyhow::Context;
use gtk::prelude::GtkWindowExt;
use gtk_layer_shell::LayerShell;
use smabar_core::config::SettingsWindowConfig;
use smabar_core::platform::WindowBackend;
use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

use super::window::backend;
use crate::platform::display::DisplaySnapshot;
use crate::surfaces::{SurfaceRole, clamp_settings_size, settings_surface_origin};

pub(super) fn resize_settings_surface(
    window: &WebviewWindow,
    size: LogicalSize<u32>,
) -> anyhow::Result<()> {
    let resized = window.clone();
    window
        .run_on_main_thread(move || match resized.gtk_window() {
            Ok(gtk_window) => {
                gtk_window.resize(size.width as i32, size.height as i32);
            }
            Err(error) => tracing::error!(%error, "failed to size settings surface"),
        })
        .context("failed to schedule settings surface resize")
}

pub fn set_surface_monitor(
    window: &WebviewWindow,
    monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    if backend() != WindowBackend::WaylandLayerShell
        || window.label() == SurfaceRole::Settings.label()
    {
        return Ok(());
    }
    let target = monitor.clone();
    let moved = window.clone();
    window
        .run_on_main_thread(move || match moved.gtk_window() {
            Ok(gtk_window) => match super::monitor::find(&target) {
                Some(native) => gtk_window.set_monitor(&native),
                None => tracing::error!(
                    monitor = target.id,
                    surface = moved.label(),
                    "selected Wayland monitor disappeared before assignment"
                ),
            },
            Err(error) => tracing::error!(%error, surface = moved.label(), "failed to assign Wayland surface monitor"),
        })
        .context("failed to schedule Wayland monitor assignment")
}

/// Sizes the settings window and puts it where the user left it, or in the
/// middle of the work area when that spot is no longer reachable.
pub fn place_settings_surface(
    window: &WebviewWindow,
    monitor: &DisplaySnapshot,
    settings: SettingsWindowConfig,
) -> anyhow::Result<()> {
    let size = clamp_settings_size(Some((settings.width, settings.height)));
    resize_settings_surface(window, size)?;
    if !crate::platform::settings_position_supported() {
        // Wayland owns placement of ordinary application windows. Native
        // move/resize gestures still work through the compositor.
        return Ok(());
    }
    let origin = settings_surface_origin(monitor, settings, size);
    window
        .set_position(PhysicalPosition::new(origin.x, origin.y))
        .context("failed to place settings surface")
}
