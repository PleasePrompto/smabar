//! Basic windows for targets other than Linux and Windows. macOS shares these
//! windows and unsupported capture/input shaping, but has its own reservation.

pub mod capture {
    use smabar_core::capture::BarError;
    use smabar_core::platform::Rect;
    use tauri::WebviewWindow;

    /// Screenshots need a platform snapshot API for the target webview. The
    /// fallback currently provides none; native Windows capture lives in its
    /// own platform module.
    pub async fn snapshot_png(
        _window: &WebviewWindow,
        _rect: Rect,
        _scale_factor: f64,
        _scale: u8,
    ) -> Result<(Vec<u8>, u32, u32), BarError> {
        Err(BarError::Unsupported)
    }
}

pub mod window {
    use anyhow::Context;
    use smabar_core::config::{BarPosition, SettingsWindowConfig};
    use smabar_core::platform::WindowLevel;
    use smabar_core::platform::surfaces::{ScreenRect, offset_bar_surface};
    use tauri::{
        AppHandle, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    };

    use crate::platform::display::DisplaySnapshot;
    use crate::surfaces::{
        SETTINGS_MIN_SIZE, SurfaceRole, clamp_settings_size, settings_surface_origin,
    };

    pub fn create_surface(
        app: &AppHandle,
        role: SurfaceRole,
        _embed_origin: Option<&str>,
        settings_size: Option<(u32, u32)>,
        settings_title: &str,
    ) -> anyhow::Result<WebviewWindow> {
        if role == SurfaceRole::Bar {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|config| config.label == role.label())
                .context("window `bar` missing from Tauri config")?;
            return WebviewWindowBuilder::from_config(app, config)
                .context("invalid `bar` window config")?
                // A macOS dock shortcut must act on the first click from another app.
                .accept_first_mouse(true)
                .build()
                .context("failed to create `bar` window");
        }
        let builder =
            WebviewWindowBuilder::new(app, role.label(), WebviewUrl::App("index.html".into()))
                .visible(false)
                .accept_first_mouse(role.plugin_capable())
                .transparent(role.plugin_capable())
                .decorations(false);
        let builder = if role == SurfaceRole::Settings {
            let size = clamp_settings_size(settings_size);
            builder
                .title(settings_title)
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
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(false)
                .resizable(false)
                .inner_size(420.0, 720.0)
        };
        builder
            .build()
            .with_context(|| format!("failed to create `{}` surface", role.label()))
    }

    /// Covers the monitor work area and shows the window. The layer-shell
    /// half of the Linux setup has no equivalent here yet, so the level is
    /// applied solely through [`apply_window_level`] (called after setup).
    pub fn setup_surface(
        window: &WebviewWindow,
        role: SurfaceRole,
        _monitor: &DisplaySnapshot,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        if matches!(role, SurfaceRole::Bar | SurfaceRole::Overlay) {
            crate::platform::macos::window::install_hover(window, role)?;
        }
        if role != SurfaceRole::Bar {
            return Ok(());
        }
        window.show().context("failed to show bar window")?;
        Ok(())
    }

    pub fn set_surface_monitor(
        _window: &WebviewWindow,
        _monitor: &DisplaySnapshot,
    ) -> anyhow::Result<()> {
        Ok(())
    }

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
            window
                .show()
                .context("failed to reveal bar after relocation")?;
        } else {
            window
                .hide()
                .context("failed to conceal bar before relocation")?;
        }
        Ok(true)
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

    /// Applies the configured stacking; top and overlay share the fallback's
    /// best available always-on-top flag.
    pub fn apply_window_level(window: &WebviewWindow, level: WindowLevel) -> anyhow::Result<()> {
        let flags = match level {
            WindowLevel::Bottom => (false, true),
            WindowLevel::Panel | WindowLevel::Top => (true, false),
        };
        crate::platform::set_window_stacking_flags(window, flags.0, flags.1)
    }

    pub fn focus(window: &WebviewWindow) -> tauri::Result<()> {
        window.set_focus()
    }
}

pub mod input_shape {
    use smabar_core::platform::Rect;

    /// Tauri command: the shell reports the clickable regions in CSS pixels.
    /// Without platform input shaping the whole window stays clickable and
    /// clicks outside the bar never fall through to the desktop.
    #[tauri::command]
    pub fn set_input_shape(rects: Vec<Rect>) -> Result<(), String> {
        warn_once(rects.len());
        Ok(())
    }

    fn warn_once(rect_count: usize) {
        static WARN_ONCE: std::sync::Once = std::sync::Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!(
                rect_count,
                "input shaping is not implemented on this platform; \
                 clicks outside the bar will not pass through"
            );
        });
    }
}

#[cfg(not(target_os = "macos"))]
pub mod strut {
    use smabar_core::platform::Rect;
    use tauri::{PhysicalPosition, WebviewWindow};

    /// Docked edge the bar occupies; `None` clears the reservation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DockEdge {
        Top,
        Bottom,
    }

    /// Space reservation is not implemented on this platform — other windows
    /// may overlap the bar. Logged once, then a silent no-op.
    pub fn apply(
        _window: &WebviewWindow,
        dock: Option<(DockEdge, Rect)>,
        _target_origin: Option<PhysicalPosition<i32>>,
    ) -> anyhow::Result<()> {
        static WARN_ONCE: std::sync::Once = std::sync::Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!(
                ?dock,
                "space reservation is not implemented on this platform"
            );
        });
        Ok(())
    }
}
