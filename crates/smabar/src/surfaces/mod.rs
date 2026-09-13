pub mod bar;
mod focus;
mod lifecycle;
pub mod menu;
mod model;
pub mod monitor;
pub mod notifications;
pub mod overlay;
mod plugin_ui;
mod pointer;
pub mod presentation;
mod settings;
pub mod tooltip;

pub(crate) use model::OverlayFlyoutRequest;
pub use model::SurfaceRole;
pub(crate) use model::SurfaceSize;
pub(crate) use pointer::install_watchdog as install_pointer_watchdog;
pub use settings::{
    SETTINGS_MIN_SIZE, clamp_settings_size, remember_settings_geometry_at_exit,
    settings_surface_origin, settings_window_title,
};

use std::sync::Mutex;

use anyhow::Context;
use model::{SettingsRequest, SurfaceContext};
use smabar_core::config::BarPosition;
use smabar_core::platform::Rect;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::commands::AppState;

use lifecycle::Lifecycle;

/// Owns the lifecycle of every shell webview. Platform-specific construction
/// remains in `platform::{linux,windows,fallback}::window`.
pub struct SurfaceManager {
    lifecycle: Mutex<Lifecycle>,
    embed_origin: Option<String>,
    settings_title: String,
}

impl SurfaceManager {
    pub fn new(embed_origin: Option<String>, settings_title: String) -> Self {
        let lifecycle = Lifecycle {
            bar_revealed: true,
            bar_visible_height: 8,
            ..Lifecycle::default()
        };
        Self {
            lifecycle: Mutex::new(lifecycle),
            embed_origin,
            settings_title,
        }
    }

    pub fn create_bar(&self, app: &AppHandle) -> anyhow::Result<WebviewWindow> {
        self.initialize_monitors(app)?;
        let window = self.ensure(app, SurfaceRole::Bar, None)?;
        let area = self.active_monitor()?.work_area;
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .bar_work_area = Some(area);
        Ok(window)
    }

    pub fn prewarm_overlay(&self, app: &AppHandle) -> anyhow::Result<()> {
        self.ensure(app, SurfaceRole::Overlay, None).map(|_| ())
    }

    fn ensure(
        &self,
        app: &AppHandle,
        role: SurfaceRole,
        settings_size: Option<(u32, u32)>,
    ) -> anyhow::Result<WebviewWindow> {
        if let Some(window) = app.get_webview_window(role.label()) {
            return Ok(window);
        }
        // Layer-shell setup is GTK and therefore main-thread only, while async
        // commands run on tokio workers. The runtime executes the task inline
        // when this already is the main thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = app.clone();
        app.run_on_main_thread(move || {
            let result = handle
                .state::<SurfaceManager>()
                .create(&handle, role, settings_size);
            // A failed send only means the caller stopped waiting.
            let _ = tx.send(result);
        })
        .context("failed to dispatch surface creation to the main thread")?;
        rx.recv()
            .context("surface creation on the main thread did not report back")?
    }

    fn create(
        &self,
        app: &AppHandle,
        role: SurfaceRole,
        settings_size: Option<(u32, u32)>,
    ) -> anyhow::Result<WebviewWindow> {
        if let Some(window) = app.get_webview_window(role.label()) {
            return Ok(window);
        }
        let window = crate::platform::window::create_surface(
            app,
            role,
            self.embed_origin.as_deref(),
            settings_size,
            &self.settings_title,
        )?;
        let monitor = self.active_monitor()?;
        if role == SurfaceRole::Bar {
            let position = app.state::<AppState>().config().layout.position;
            crate::platform::place_bar_bootstrap(&window, position, &monitor)?;
        }
        crate::platform::window::setup_surface(&window, role, &monitor)?;
        if role != SurfaceRole::Bar {
            crate::platform::hide_surface(&window)
                .with_context(|| format!("failed to hide prewarmed `{}` surface", role.label()))?;
        }
        tracing::info!(surface = role.label(), "surface created");
        Ok(window)
    }

    pub fn open_settings(
        &self,
        app: &AppHandle,
        group: &str,
        size: (u32, u32),
    ) -> anyhow::Result<()> {
        let group = settings::normalize_settings_group(group).to_string();
        let window = self.ensure(app, SurfaceRole::Settings, Some(size))?;
        let previous = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.settings_group.replace(group)
        };
        let result = self.is_ready(SurfaceRole::Settings).and_then(|ready| {
            if ready {
                self.present_settings(&window)
            } else {
                Ok(())
            }
        });
        if result.is_err() {
            self.lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .settings_group = previous;
        }
        result.and(self.report_settings_state(app))
    }

    pub fn toggle_settings(
        &self,
        app: &AppHandle,
        group: &str,
        size: (u32, u32),
    ) -> anyhow::Result<()> {
        let opening = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .settings_group
            .is_some();
        if let Some(window) = app.get_webview_window(SurfaceRole::Settings.label())
            && opening
        {
            // The gear toggles only the window in front; one that slipped
            // behind another window comes back instead of vanishing.
            if window.is_focused().unwrap_or(true) {
                return self.close(app, SurfaceRole::Settings);
            }
            return settings::raise_settings(&window);
        }
        self.open_settings(app, group, size)
    }

    fn present_settings(&self, window: &WebviewWindow) -> anyhow::Result<()> {
        let group = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .settings_group
            .clone()
            .unwrap_or_else(|| "bar".to_string());
        let config = window.app_handle().state::<AppState>().config();
        tracing::debug!(surface = window.label(), %group, "presenting settings");
        let monitor = self.active_monitor()?;
        if !window
            .is_visible()
            .context("failed to read settings visibility")?
        {
            crate::platform::window::place_settings_surface(
                window,
                &monitor,
                config.settings_window,
            )?;
        }
        window
            .emit("surface-settings", SettingsRequest { group })
            .context("failed to send settings surface state")?;
        settings::raise_settings(window)
    }

    pub(crate) fn is_ready(&self, role: SurfaceRole) -> anyhow::Result<bool> {
        self.lifecycle
            .lock()
            .map(|state| state.ready.contains(&role))
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))
    }

    pub fn mark_ready(&self, app: &AppHandle, role: SurfaceRole) -> anyhow::Result<()> {
        let (popups, notice, flyout, menu, tooltip) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.ready.insert(role);
            if role == SurfaceRole::Notifications {
                (
                    std::mem::take(&mut lifecycle.pending_popups),
                    lifecycle.pending_notice.take(),
                    None,
                    None,
                    None,
                )
            } else if role == SurfaceRole::Overlay {
                (
                    Vec::new(),
                    None,
                    lifecycle
                        .active_flyout
                        .as_ref()
                        .map(|active| active.request.clone()),
                    lifecycle
                        .active_menu
                        .as_ref()
                        .map(|active| active.request.clone()),
                    lifecycle
                        .active_tooltip
                        .as_ref()
                        .map(|active| active.request.clone()),
                )
            } else {
                (Vec::new(), None, None, None, None)
            }
        };
        tracing::debug!(surface = role.label(), "surface ready");
        if role == SurfaceRole::Bar && crate::platform::needs_bar_pointer_watchdog() {
            self.report_bar_pointer(app)?;
        }
        if role == SurfaceRole::Settings
            && let Some(window) = app.get_webview_window(role.label())
        {
            let requested = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .settings_group
                .is_some();
            if requested {
                self.present_settings(&window)?;
            } else {
                crate::platform::hide_surface(&window)
                    .context("failed to hide the prewarmed settings surface after startup")?;
            }
        }
        if role == SurfaceRole::Notifications
            && let Some(window) = app.get_webview_window(role.label())
        {
            for popup in popups {
                window
                    .emit("plugin-ui", popup)
                    .context("failed to deliver buffered plugin popup")?;
            }
            if let Some(notice) = notice {
                window
                    .emit("surface-notice", notice)
                    .context("failed to deliver buffered shell notice")?;
            }
        }
        if role == SurfaceRole::Overlay
            && let Some(window) = app.get_webview_window(role.label())
        {
            if let Some(flyout) = flyout {
                self.deliver_flyout(app, &flyout, true)?;
            }
            if let Some(menu) = menu {
                window
                    .emit("surface-menu", menu)
                    .context("failed to deliver buffered context menu")?;
            }
            if let Some(tooltip) = tooltip {
                window
                    .emit("surface-tooltip", tooltip)
                    .context("failed to deliver buffered tooltip")?;
            }
        }
        Ok(())
    }

    pub fn close(&self, app: &AppHandle, role: SurfaceRole) -> anyhow::Result<()> {
        if role == SurfaceRole::Settings {
            if let Some(window) = app.get_webview_window(role.label()) {
                if window.is_visible().unwrap_or(false)
                    && let Err(error) = settings::remember_settings_geometry(app, &window)
                {
                    tracing::warn!(%error, "settings window geometry was not saved; it opens centered next time");
                }
                crate::platform::hide_surface(&window)
                    .context("failed to hide settings surface")?;
            }
            self.lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .settings_group = None;
            tracing::debug!(surface = role.label(), "surface hidden");
            return self
                .report_bar_pointer(app)
                .and(self.report_settings_state(app));
        }
        if role.persistent() {
            anyhow::bail!("persistent surface `{}` cannot be closed", role.label());
        }
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
            .ready
            .remove(&role);
        if role == SurfaceRole::Notifications {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            lifecycle.notification_measure = None;
            lifecycle.pending_popups.clear();
            lifecycle.pending_notice = None;
        }
        if let Some(window) = app.get_webview_window(role.label()) {
            window
                .destroy()
                .with_context(|| format!("failed to destroy `{}` surface", role.label()))?;
        }
        tracing::info!(surface = role.label(), "surface closed");
        Ok(())
    }

    pub fn window_destroyed(&self, app: &AppHandle, label: &str) {
        let Some(role) = SurfaceRole::from_label(label) else {
            return;
        };
        let Ok(mut lifecycle) = self.lifecycle.lock() else {
            tracing::error!(
                surface = label,
                "surface lifecycle lock poisoned after destroy"
            );
            return;
        };
        lifecycle.ready.remove(&role);
        if role == SurfaceRole::Settings {
            lifecycle.settings_group = None;
        }
        if role == SurfaceRole::Notifications {
            lifecycle.notification_measure = None;
        }
        if role == SurfaceRole::Overlay {
            lifecycle.flyout_layout = None;
            lifecycle.menu_frame = None;
            lifecycle.tooltip_layout = None;
            lifecycle.overlay_frame = None;
        }
        drop(lifecycle);
        if role == SurfaceRole::Settings
            && let Err(error) = self
                .report_bar_pointer(app)
                .and(self.report_settings_state(app))
        {
            tracing::error!(%error, "failed to release the bar after Settings was destroyed");
        }
    }

    pub fn context(&self, window: &WebviewWindow) -> anyhow::Result<SurfaceContext> {
        let role = SurfaceRole::from_label(window.label())
            .with_context(|| format!("unknown surface label `{}`", window.label()))?;
        let monitor = self.active_monitor()?;
        let area = monitor.work_area;
        let scale = monitor.scale_factor;
        Ok(SurfaceContext {
            role,
            work_area_width: f64::from(area.w) / scale,
            work_area_height: f64::from(area.h) / scale,
            settings_open: self.settings_open()?,
        })
    }

    pub(crate) fn bar_geometry(&self, position: BarPosition) -> anyhow::Result<Option<Rect>> {
        self.lifecycle
            .lock()
            .map(|state| {
                state
                    .bar_geometry
                    .filter(|(saved, _)| *saved == position)
                    .map(|(_, rect)| rect)
            })
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))
    }
}

#[tauri::command]
pub fn get_surface_context(
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
) -> Result<SurfaceContext, String> {
    manager
        .context(&window)
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn surface_ready(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
    device_pixel_ratio: f64,
) -> Result<(), String> {
    let role = SurfaceRole::from_label(window.label())
        .ok_or_else(|| format!("unknown surface label `{}`", window.label()))?;
    crate::platform::normalize_webview_scale(&window, device_pixel_ratio)
        .map_err(|error| format!("{error:#}"))?;
    manager
        .mark_ready(&app, role)
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn open_settings(app: AppHandle, group: Option<String>) -> Result<(), String> {
    open_settings_from_app(&app, group.as_deref().unwrap_or("bar"))
        .map_err(|error| format!("{error:#}"))
}

pub fn open_settings_from_app(app: &AppHandle, group: &str) -> anyhow::Result<()> {
    let config = app.state::<AppState>().config();
    app.state::<SurfaceManager>().open_settings(
        app,
        group,
        (config.settings_window.width, config.settings_window.height),
    )
}

#[tauri::command]
pub async fn toggle_settings(app: AppHandle) -> Result<(), String> {
    let config = app.state::<AppState>().config();
    app.state::<SurfaceManager>()
        .toggle_settings(
            &app,
            "bar",
            (config.settings_window.width, config.settings_window.height),
        )
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn close_settings(app: AppHandle, manager: State<'_, SurfaceManager>) -> Result<(), String> {
    manager
        .close(&app, SurfaceRole::Settings)
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn close_surface(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, SurfaceManager>,
) -> Result<(), String> {
    let role = SurfaceRole::from_label(window.label())
        .ok_or_else(|| format!("unknown surface label `{}`", window.label()))?;
    manager
        .close(&app, role)
        .map_err(|error| format!("{error:#}"))
}
