//! Windows window setup and stacking.

use anyhow::Context;
use smabar_core::config::{BarPosition, SettingsWindowConfig};
use smabar_core::platform::WindowLevel;
use smabar_core::platform::surfaces::{ScreenRect, offset_bar_surface};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use super::input_shape;
use crate::commands::AppState;
use crate::platform::display::DisplaySnapshot;
use crate::surfaces::{
    SETTINGS_MIN_SIZE, SurfaceRole, clamp_settings_size, settings_surface_origin,
};

pub fn create_surface(
    app: &AppHandle,
    role: SurfaceRole,
    embed_origin: Option<&str>,
    settings_size: Option<(u32, u32)>,
    settings_title: &str,
) -> anyhow::Result<WebviewWindow> {
    let window = if role == SurfaceRole::Bar {
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == role.label())
            .context("window `bar` missing from Tauri config")?;
        WebviewWindowBuilder::from_config(app, config)
            .context("invalid `bar` window config")?
            .build()
            .context("failed to create `bar` window")?
    } else {
        let builder =
            WebviewWindowBuilder::new(app, role.label(), WebviewUrl::App("index.html".into()))
                .visible(false)
                .decorations(false);
        let builder = if role == SurfaceRole::Settings {
            // A normal top-level window: opaque, in the taskbar, never above
            // other windows, minimum size enforced by the system.
            let size = clamp_settings_size(settings_size);
            builder
                .title(settings_title)
                .transparent(false)
                .always_on_top(false)
                .skip_taskbar(false)
                .shadow(true)
                .resizable(true)
                .min_inner_size(
                    f64::from(SETTINGS_MIN_SIZE.0),
                    f64::from(SETTINGS_MIN_SIZE.1),
                )
                .inner_size(f64::from(size.width), f64::from(size.height))
        } else {
            builder
                .title(format!("smabar {}", role.label()))
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(false)
                .resizable(false)
                .inner_size(420.0, 720.0)
        };
        builder
            .build()
            .with_context(|| format!("failed to create `{}` surface", role.label()))?
    };
    if role.plugin_capable()
        && let Some(origin) = embed_origin
    {
        super::provider_identity::install(&window, origin)
            .context("failed to install the WebView2 provider identity hook")?;
    }
    Ok(window)
}

pub fn setup_surface(
    window: &WebviewWindow,
    role: SurfaceRole,
    _monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    if role != SurfaceRole::Bar {
        return Ok(());
    }
    super::native::install(window)?;
    input_shape::apply(window, Vec::new())?;
    window.show().context("failed to show bar window")?;
    Ok(())
}

pub fn set_surface_monitor(
    _window: &WebviewWindow,
    _monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    Ok(())
}

/// Sizes the settings window and puts it where the user left it, or in the
/// middle of the work area when that spot is no longer reachable.
pub fn place_settings_surface(
    window: &WebviewWindow,
    monitor: &DisplaySnapshot,
    settings: SettingsWindowConfig,
) -> anyhow::Result<()> {
    let size = clamp_settings_size(Some((settings.width, settings.height)));
    let origin = settings_surface_origin(monitor, settings, size);
    window
        .set_position(PhysicalPosition::new(origin.x, origin.y))
        .context("failed to place settings surface")?;
    window
        .set_size(size)
        .context("failed to size settings surface")
}

pub async fn set_bar_transition_opaque(
    window: &WebviewWindow,
    opaque: bool,
) -> anyhow::Result<bool> {
    if opaque {
        show_surface(window)?;
    } else {
        stage_surface(window)?;
    }
    Ok(true)
}

/// Tauri's window hide only changes the parent HWND. Hide its webview too so
/// WebView2 can throttle rendering and release caches while the surface is idle.
pub fn hide_surface(window: &WebviewWindow) -> anyhow::Result<()> {
    window.hide().context("failed to hide surface window")?;
    window
        .as_ref()
        .hide()
        .context("failed to hide surface webview")
}

/// Resume the child webview before exposing its parent HWND.
pub fn show_surface(window: &WebviewWindow) -> anyhow::Result<()> {
    window
        .as_ref()
        .show()
        .context("failed to show surface webview")?;
    window.show().context("failed to show surface window")
}

/// A replacement must measure and paint while its parent stays concealed.
/// Wry's webview show affects its child HWND, not the top-level window.
pub fn stage_surface(window: &WebviewWindow) -> anyhow::Result<()> {
    window
        .hide()
        .context("failed to conceal surface before replacement")?;
    window
        .as_ref()
        .show()
        .context("failed to resume surface webview for replacement")
}

/// Settings retain capturable DOM while closed. Wake only their child webview;
/// the parent HWND stays hidden throughout a background screenshot.
pub fn set_capture_webview_active(window: &WebviewWindow, active: bool) -> anyhow::Result<()> {
    if window.label() != SurfaceRole::Settings.label() {
        return Ok(());
    }
    if active {
        window
            .as_ref()
            .show()
            .context("failed to wake settings webview for capture")
    } else {
        window
            .as_ref()
            .hide()
            .context("failed to restore hidden settings webview after capture")
    }
}

pub fn place_bar_surface(
    window: &WebviewWindow,
    frame: ScreenRect,
    _monitor_origin: PhysicalPosition<i32>,
    _scale: f64,
    position: BarPosition,
) -> anyhow::Result<()> {
    window
        .set_size(PhysicalSize::new(frame.w, frame.h))
        .context("failed to size bar surface")?;
    move_bar_surface(
        window,
        frame,
        PhysicalPosition::new(0, 0),
        1.0,
        position,
        0,
        false,
    )
}

pub fn move_bar_surface(
    window: &WebviewWindow,
    frame: ScreenRect,
    _monitor_origin: PhysicalPosition<i32>,
    _scale: f64,
    position: BarPosition,
    offset: u32,
    _at_hidden_edge: bool,
) -> anyhow::Result<()> {
    let frame = offset_bar_surface(frame, position == BarPosition::Top, offset);
    super::placement::record(frame.x, frame.y);
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .context("failed to move bar surface")
}

pub async fn place_surface(
    window: &WebviewWindow,
    role: SurfaceRole,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    _monitor_origin: PhysicalPosition<i32>,
    _scale: f64,
    _keyboard: bool,
) -> anyhow::Result<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let placed = window.clone();
    window
        .run_on_main_thread(move || {
            let result = placed
                .set_size(size)
                .and_then(|()| placed.set_position(position))
                .map_err(|error| format!("{error:#}"));
            let _ = send.send(result);
        })
        .with_context(|| format!("failed to schedule `{}` placement", role.label()))?;
    receive
        .await
        .with_context(|| format!("`{}` placement callback was dropped", role.label()))?
        .map_err(anyhow::Error::msg)
}

pub fn apply_window_level(window: &WebviewWindow, level: WindowLevel) -> anyhow::Result<()> {
    let flags = match level {
        WindowLevel::Bottom => (false, true),
        WindowLevel::Panel | WindowLevel::Top => (true, false),
    };
    let bar = window.clone();
    window
        .run_on_main_thread(move || {
            let stacking = crate::platform::set_window_stacking_flags(&bar, flags.0, flags.1);
            // Tao may have rewritten the complete extended-style word even if
            // a later stacking call failed. Always restore the input-routing
            // bits in the same ordered UI-thread operation.
            let routing = super::hit_test::refresh_cursor();
            if let Err(error) = stacking {
                tracing::error!(
                    %error,
                    ?level,
                    "failed to apply the Windows window level; stacking may remain stale until the next window-level update"
                );
            }
            if let Err(error) = routing {
                tracing::error!(
                    %error,
                    ?level,
                    "failed to refresh Windows input routing after a window-level update; pointer input may be routed incorrectly until the next mouse event"
                );
            }
        })
        .context("failed to schedule the Windows window-level update")
}

pub(super) fn apply_fullscreen_signal(app: &AppHandle, active: bool) {
    let state = app.state::<AppState>();
    if !state.set_fullscreen_active(active) {
        return;
    }
    let level = state.window_level();
    if let Some(window) = app.get_webview_window("bar")
        && let Err(error) = apply_window_level(&window, level)
    {
        tracing::error!(
            %error,
            active,
            ?level,
            "failed to apply the fullscreen window-level change; bar stacking may remain stale until the next surface or configuration update"
        );
    }
    tracing::info!(
        active,
        ?level,
        "Windows fullscreen application state changed"
    );
}

pub fn focus(window: &WebviewWindow) -> tauri::Result<()> {
    window.set_focus()
}
