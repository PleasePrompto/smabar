//! The settings window as a normal top-level window: its remembered
//! geometry, its title, and the group ids the shell navigates by.

use anyhow::Context;
use smabar_core::config::SettingsWindowConfig;
use smabar_core::i18n::LocaleMap;
use smabar_core::platform::settings::settings_origin;
use smabar_core::platform::surfaces::ScreenPoint;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewWindow};

use super::{SurfaceManager, SurfaceRole};
use crate::commands::AppState;
use crate::platform::display::DisplaySnapshot;

impl SurfaceManager {
    /// Queue restoration under the same lock that records a settings open.
    /// A concurrent reopen then queues its show afterwards, never before a
    /// stale capture hide. Monitor staging also keeps settings_group present.
    pub(crate) fn restore_hidden_settings_capture(
        &self,
        window: &WebviewWindow,
    ) -> anyhow::Result<()> {
        if window.label() != SurfaceRole::Settings.label() {
            return Ok(());
        }
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        if lifecycle.settings_group.is_none() {
            crate::platform::set_capture_webview_active(window, false)?;
        }
        Ok(())
    }

    pub(super) fn settings_open(&self) -> anyhow::Result<bool> {
        self.lifecycle
            .lock()
            .map(|state| state.settings_group.is_some())
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))
    }

    pub(super) fn report_settings_state(&self, app: &AppHandle) -> anyhow::Result<()> {
        app.emit_to(
            SurfaceRole::Bar.label(),
            "settings-open-changed",
            self.settings_open()?,
        )
        .context("failed to report Settings visibility to the bar")
    }
}

/// Smallest usable settings window in logical pixels; the native builders
/// hand it to the window manager as the minimum size.
pub const SETTINGS_MIN_SIZE: (u32, u32) = (640, 480);

/// Gives native resize callbacks a safe lower bound before the shell's CSS
/// constraints take over. Logical pixels, like the config.
pub fn clamp_settings_size(size: Option<(u32, u32)>) -> LogicalSize<u32> {
    let (width, height) = size.unwrap_or((
        SettingsWindowConfig::default().width,
        SettingsWindowConfig::default().height,
    ));
    LogicalSize::new(
        width.max(SETTINGS_MIN_SIZE.0),
        height.max(SETTINGS_MIN_SIZE.1),
    )
}

/// Physical top-left corner for the settings window on `monitor`: the
/// remembered spot while it is still reachable there, otherwise centered.
pub fn settings_surface_origin(
    monitor: &DisplaySnapshot,
    settings: SettingsWindowConfig,
    size: LogicalSize<u32>,
) -> ScreenPoint {
    let scale = monitor.scale_factor;
    let physical = |value: u32| (f64::from(value) * scale).round() as u32;
    let remembered = settings.x.zip(settings.y).map(|(x, y)| ScreenPoint {
        x: (f64::from(x) * scale).round() as i32,
        y: (f64::from(y) * scale).round() as i32,
    });
    settings_origin(
        remembered,
        (physical(size.width), physical(size.height)),
        monitor.work_area,
    )
}

/// The title the window list and Alt-Tab show for the settings window.
pub fn settings_window_title(locale: &LocaleMap) -> String {
    let title = locale
        .get("settings.title")
        .map_or("Settings", String::as_str);
    format!("{title} – smabar")
}

/// Writes the window's current size and position into the config so the
/// next open lands where the user left it. Read while the window is still
/// mapped: a hidden window reports stale geometry.
pub(super) fn remember_settings_geometry(
    app: &AppHandle,
    window: &WebviewWindow,
) -> anyhow::Result<()> {
    let scale = window
        .scale_factor()
        .context("failed to read the settings window scale factor")?;
    let size = window
        .inner_size()
        .context("failed to read the settings window size")?
        .to_logical::<u32>(scale);
    // Wayland does not expose absolute application-window coordinates.
    let position = if crate::platform::settings_position_supported() {
        Some(
            window
                .outer_position()
                .context("failed to read the settings window position")?
                .to_logical::<i32>(scale),
        )
    } else {
        None
    };
    let geometry = SettingsWindowConfig {
        width: size.width.max(SETTINGS_MIN_SIZE.0),
        height: size.height.max(SETTINGS_MIN_SIZE.1),
        x: position.map(|point| point.x),
        y: position.map(|point| point.y),
    };
    app.state::<AppState>()
        .remember_settings_window(geometry)
        .context("failed to write the settings window geometry")
}

/// Saves the geometry of a settings window that is still open when smabar
/// quits; a closed one was saved when it closed.
pub fn remember_settings_geometry_at_exit(app: &AppHandle) {
    let Some(window) = app.get_webview_window(SurfaceRole::Settings.label()) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    if let Err(error) = remember_settings_geometry(app, &window) {
        tracing::warn!(%error, "settings window geometry was not saved on exit; it opens centered next time");
    }
}

/// Brings an open settings window that slipped behind other windows back to
/// the front, without moving it.
pub(super) fn raise_settings(window: &WebviewWindow) -> anyhow::Result<()> {
    window
        .unminimize()
        .context("failed to unminimize the settings window")?;
    crate::platform::show_surface(window).context("failed to show the settings window")?;
    window
        .set_focus()
        .context("failed to focus the settings window")
}

/// The settings groups and pages the shell knows; anything else opens the
/// bar group rather than an empty panel.
pub(super) fn normalize_settings_group(group: &str) -> &'static str {
    match group {
        "design" => "design",
        "shortcuts" => "shortcuts",
        // `tiles` is the pre-rename spelling agents may still send.
        "plugins" | "tiles" => "plugins",
        "plugins/store" => "plugins/store",
        "design/themes" => "design/themes",
        "system" | "info" => "system",
        "legal" => "legal",
        _ => "bar",
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_settings_size, normalize_settings_group, settings_window_title};

    #[test]
    fn settings_inputs_are_bounded_and_normalized() {
        assert_eq!(normalize_settings_group("info"), "system");
        assert_eq!(normalize_settings_group("design"), "design");
        assert_eq!(normalize_settings_group("plugins"), "plugins");
        assert_eq!(normalize_settings_group("tiles"), "plugins");
        assert_eq!(normalize_settings_group("plugins/store"), "plugins/store");
        assert_eq!(normalize_settings_group("design/themes"), "design/themes");
        assert_eq!(normalize_settings_group("legal"), "legal");
        assert_eq!(normalize_settings_group("../../bad"), "bar");
        let size = clamp_settings_size(Some((1, 2)));
        assert_eq!((size.width, size.height), (640, 480));
        let size = clamp_settings_size(None);
        assert_eq!((size.width, size.height), (960, 680));
    }

    #[test]
    fn window_title_follows_the_locale() {
        let mut locale = smabar_core::i18n::LocaleMap::new();
        assert_eq!(settings_window_title(&locale), "Settings – smabar");
        locale.insert("settings.title".into(), "Einstellungen".into());
        assert_eq!(settings_window_title(&locale), "Einstellungen – smabar");
    }
}
