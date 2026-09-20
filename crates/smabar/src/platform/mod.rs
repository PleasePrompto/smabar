//! Per-platform window integration — the one sanctioned home for platform
//! `cfg`s in this crate (see CLAUDE.md). Linux and Windows carry native dock
//! integrations; every other target gets [`fallback`], a plain always-on-top
//! window until its native integration lands.

pub mod audio;
pub mod autostart;
#[cfg(any(target_os = "linux", windows, test))]
mod capture_image;
pub mod display;
pub mod reservation_access;
#[cfg(not(feature = "no-self-update"))]
mod update;

#[cfg(not(feature = "no-self-update"))]
pub use update::{InstallMode, install_mode, updater_target};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{capture, input_shape, strut, window};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{capture, input_shape, strut, window};

// Compiled on Linux during unit tests so the Windows coordinate and AppBar
// math stays covered without a cross target or native calls.
#[cfg(any(windows, test))]
#[path = "windows/geometry.rs"]
mod windows_geometry;

// Compiled on Linux during unit tests so watchdog detection, retry, and log
// deduplication decisions stay deterministic without native input APIs.
#[cfg(any(windows, test))]
#[path = "windows/watchdog.rs"]
mod windows_watchdog;

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
pub use fallback::{capture, input_shape, strut, window};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use fallback::{capture, input_shape, window};
#[cfg(target_os = "macos")]
pub use macos::strut;
#[cfg(any(target_os = "macos", test))]
#[path = "macos/geometry.rs"]
mod macos_geometry;

use anyhow::Context;
use smabar_core::config::{BarPosition, RenderingMode};
use smabar_core::platform::render::RenderPlan;
use smabar_core::platform::surfaces::bar_surface_frame;
use tauri::{Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

pub fn check_webview(
    _paths: &smabar_core::config::SmabarPaths,
    _language: &str,
    _minimum: Option<&str>,
) -> anyhow::Result<()> {
    #[cfg(windows)]
    windows::runtime::check(_paths, _language, _minimum)?;
    Ok(())
}

fn native_monitor_identities(monitors: &[Monitor]) -> Vec<display::NativeIdentity> {
    #[cfg(target_os = "linux")]
    {
        linux::monitor::identities(monitors)
    }
    #[cfg(windows)]
    {
        windows::monitor::identities(monitors)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = monitors;
        Vec::new()
    }
}

const fn monitor_id_prefix() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        "other"
    }
}

/// Selects native shortcut discovery, icon lookup and opening at the app edge.
pub fn shortcut_platform(
    home: &std::path::Path,
    icons_dir: &std::path::Path,
    language: &str,
) -> anyhow::Result<smabar_core::shortcuts::ShortcutPlatform> {
    #[cfg(windows)]
    {
        let _ = (home, language);
        smabar_core::platform::shortcuts::windows(icons_dir.to_path_buf())
            .context("failed to start the Windows shortcut worker")
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (icons_dir, language);
        Ok(smabar_core::platform::shortcuts::macos(home.to_path_buf()))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let xdg_data_home = std::env::var("XDG_DATA_HOME").ok();
        let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").ok();
        let search_dirs = smabar_core::shortcuts::default_search_dirs(
            xdg_data_home.as_deref(),
            xdg_data_dirs.as_deref(),
            home,
        );
        let mut icon_dirs = smabar_core::shortcuts::default_icon_dirs(
            xdg_data_home.as_deref(),
            xdg_data_dirs.as_deref(),
            home,
        );
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|dir| !dir.trim().is_empty())
            .map_or_else(|| home.join(".config"), std::path::PathBuf::from);
        icon_dirs.preferred_theme =
            std::fs::read_to_string(config_home.join("gtk-3.0/settings.ini"))
                .ok()
                .as_deref()
                .and_then(smabar_core::shortcuts::icon_theme_from_settings_ini);
        let _ = icons_dir;
        Ok(smabar_core::platform::shortcuts::xdg(
            search_dirs,
            icon_dirs,
            Some(language.to_string()),
            home.to_path_buf(),
        ))
    }
}

/// Decides and exports how the webview renders. Linux only: WebKitGTK reads
/// the environment when it starts its web process; WebView2 needs nothing,
/// so other targets report no plan and the shell hides the setting.
pub fn prepare_rendering(mode: RenderingMode) -> Option<RenderPlan> {
    #[cfg(target_os = "linux")]
    {
        Some(linux::render::prepare(mode))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mode;
        None
    }
}

pub fn install_monitor_watch(app: &tauri::AppHandle) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::monitor::install_watch(app)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Ok(())
    }
}

pub fn settings_position_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        !matches!(
            linux::window::backend(),
            smabar_core::platform::WindowBackend::WaylandLayerShell
                | smabar_core::platform::WindowBackend::WaylandFallback
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Only the native input target may supply hover coordinates. Query on the
/// owner thread: GTK/AppKit objects must not be accessed by the async watchdog.
pub async fn bar_pointer_sample(window: &WebviewWindow) -> anyhow::Result<Option<[f64; 2]>> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let bar = window.clone();
    window
        .run_on_main_thread(move || {
            #[cfg(target_os = "linux")]
            let sample = linux::input_shape::pointer_sample(&bar);
            #[cfg(windows)]
            let sample = windows::input_shape::pointer_sample(&bar);
            #[cfg(target_os = "macos")]
            let sample = macos::window::pointer_sample(&bar);
            #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
            let sample = {
                let _ = bar;
                Ok(None)
            };
            let _ = send.send(sample);
        })
        .context("failed to schedule native bar pointer sampling")?;
    receive
        .await
        .context("native bar pointer callback was dropped")?
}

pub fn needs_bar_pointer_watchdog() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::window::backend() == smabar_core::platform::WindowBackend::X11
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Conceals a mapped bar during an edge relocation. Keeping it mapped lets
/// WebKit continue measuring the replacement layout.
pub async fn set_bar_transition_opaque(
    window: &WebviewWindow,
    opaque: bool,
) -> anyhow::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        linux::transition::set_opaque(window, opaque).await
    }
    #[cfg(windows)]
    {
        windows::window::set_bar_transition_opaque(window, opaque).await
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        fallback::window::set_bar_transition_opaque(window, opaque).await
    }
}

/// Hides an idle surface, including its WebView2 controller on Windows.
pub fn hide_surface(window: &WebviewWindow) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::window::hide_surface(window)
    }
    #[cfg(not(windows))]
    {
        window.hide().context("failed to hide surface")
    }
}

/// Resumes a surface's webview before showing its native window.
pub fn show_surface(window: &WebviewWindow) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::window::show_surface(window)
    }
    #[cfg(not(windows))]
    {
        window.show().context("failed to show surface")
    }
}

/// Windows settings keep their DOM while closed, but their controller needs
/// to render before capture staging. Other surfaces and platforms need no change.
pub fn set_capture_webview_active(window: &WebviewWindow, active: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::window::set_capture_webview_active(window, active)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, active);
        Ok(())
    }
}

/// Conceals transient content before its DOM and native frame are replaced.
/// X11 keeps the window mapped so WebKitGTK can paint the new buffer;
/// Wayland and other platforms withdraw it until placement.
pub async fn stage_transient_update(window: &WebviewWindow) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::transition::stage_transient_update(window).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let transient = window.clone();
        window
            .run_on_main_thread(move || {
                #[cfg(windows)]
                let result = windows::window::stage_surface(&transient)
                    .map_err(|error| format!("{error:#}"));
                #[cfg(not(windows))]
                let result = transient.hide().map_err(|error| format!("{error:#}"));
                let _ = send.send(result);
            })
            .context("failed to schedule transient surface concealment")?;
        receive
            .await
            .context("transient surface concealment callback was dropped")?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
}

/// The freshness proof for a later `present_transient`/`hide_transient_after_paint`.
/// Capture it after the stage the presented content belongs to (for the shared
/// overlay: before reading the lifecycle state about to be presented). Every
/// later `stage_transient_update` invalidates it, so a reveal or hide racing a
/// newer content swap aborts instead of exposing the previous buffer.
pub fn transient_presentation_token(window: &WebviewWindow) -> anyhow::Result<u64> {
    #[cfg(target_os = "linux")]
    {
        linux::transition::presentation_token(window)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        Ok(0)
    }
}

/// Shows a fully placed transient. Linux waits for WebKitGTK's replacement
/// snapshot (plus native paint cycles on X11), and reveals it only while
/// `token` still names the newest staged content.
pub fn present_transient(window: &WebviewWindow, token: u64) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    show_surface(window).context("failed to show transient surface")?;
    #[cfg(target_os = "macos")]
    macos::window::present_transient(window)?;
    #[cfg(target_os = "linux")]
    {
        linux::transition::reveal_transient_after_paint(window, token)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = token;
        Ok(())
    }
}

/// Unmaps a transient only after its now-empty WebKit buffer was painted and
/// only while `token` still names the newest staged content.
pub fn hide_transient_after_paint(window: &WebviewWindow, token: u64) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::transition::hide_transient_after_paint(window, token)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = token;
        hide_surface(window).context("failed to hide cleared transient surface")
    }
}

/// Keeps CSS pixels aligned with native window coordinates. WebKitGTK can
/// assign a related webview a different device scale than its GTK window.
pub fn normalize_webview_scale(
    window: &WebviewWindow,
    device_pixel_ratio: f64,
) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::scale::normalize(window, device_pixel_ratio)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, device_pixel_ratio);
        Ok(())
    }
}

/// Releases native process-lifetime registrations before Tauri exits or restarts.
pub fn shutdown() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::shutdown()
    }
    #[cfg(target_os = "macos")]
    {
        macos::strut::shutdown()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(())
    }
}

/// Gives the hidden bar enough width to resolve its responsive layout while
/// keeping the first WebKit allocation far below monitor size. The shell's
/// first measurement immediately replaces this bootstrap frame.
pub(crate) fn place_bar_bootstrap(
    window: &WebviewWindow,
    position: BarPosition,
    monitor: &display::DisplaySnapshot,
) -> anyhow::Result<()> {
    let area = monitor.work_area;
    let scale = monitor.scale_factor;
    let frame = bar_surface_frame(
        area,
        (area.w, (256.0_f64 * scale).round().max(1.0) as u32),
        position == BarPosition::Top,
    );
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .context("failed to position bar window")?;
    window
        .set_size(PhysicalSize::new(frame.w, frame.h))
        .context("failed to size bar window")?;
    tracing::info!(
        ?frame,
        monitor = monitor.label,
        "bar bootstrap surface placed"
    );
    Ok(())
}

/// Applies plain native stacking flags. Wayland layer-shell maps the same
/// intent to protocol layers instead.
fn set_window_stacking_flags(
    window: &WebviewWindow,
    always_on_top: bool,
    always_on_bottom: bool,
) -> anyhow::Result<()> {
    window
        .set_always_on_top(false)
        .context("failed to clear always-on-top")?;
    window
        .set_always_on_bottom(false)
        .context("failed to clear always-on-bottom")?;
    if always_on_top {
        window
            .set_always_on_top(true)
            .context("failed to set always-on-top")?;
    } else if always_on_bottom {
        window
            .set_always_on_bottom(true)
            .context("failed to set always-on-bottom")?;
    }
    Ok(())
}
