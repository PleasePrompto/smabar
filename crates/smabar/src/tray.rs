//! Thin Tauri tray wiring.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use smabar_core::i18n::LocaleMap;
use tauri::menu::{CheckMenuItem, MenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Listener, Manager};

use crate::platform::autostart::{self, AutostartStatus};

const SETTINGS_ID: &str = "tray-settings";
const ABOUT_ID: &str = "tray-about";
const RESTART_ID: &str = "tray-restart";
const QUIT_ID: &str = "tray-quit";
const AUTOSTART_ID: &str = "tray-autostart";

#[derive(Debug, PartialEq, Eq)]
enum ClickAction {
    ShowBar,
    OpenSettings,
}

fn click_action(event: TrayIconEvent, suppress_release: &AtomicBool) -> Option<ClickAction> {
    match event {
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            // Windows follows DoubleClick with Click(Up). That release must
            // not focus the bar over the Settings window we just opened.
            suppress_release.store(true, Ordering::Relaxed);
            Some(ClickAction::OpenSettings)
        }
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Down,
            ..
        } => {
            suppress_release.store(false, Ordering::Relaxed);
            None
        }
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => (!suppress_release.swap(false, Ordering::Relaxed)).then_some(ClickAction::ShowBar),
        _ => None,
    }
}

pub fn setup(app: &App, locale: &LocaleMap) -> anyhow::Result<()> {
    let settings = locale
        .get("tray.settings")
        .context("locale is missing tray.settings")?;
    let about = locale
        .get("tray.about")
        .context("locale is missing tray.about")?;
    let tooltip = locale
        .get("tray.tooltip")
        .context("locale is missing tray.tooltip")?;
    let restart = locale
        .get("tray.restart")
        .context("locale is missing tray.restart")?;
    let quit = locale
        .get("tray.quit")
        .context("locale is missing tray.quit")?;
    let autostart_label = locale
        .get("settings.system.autostart")
        .context("locale is missing settings.system.autostart")?;
    let status = autostart::current(app.handle())?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        AUTOSTART_ID,
        autostart_label,
        status.registered().is_some(),
        status.registered().unwrap_or(false),
        None::<&str>,
    )?;
    let menu = MenuBuilder::new(app)
        .text(SETTINGS_ID, settings)
        .text(ABOUT_ID, about)
        .separator()
        .item(&autostart_item)
        .separator()
        .text(RESTART_ID, restart)
        .text(QUIT_ID, quit)
        .build()
        .context("failed to build the tray menu")?;
    let icon = app
        .default_window_icon()
        .cloned()
        .context("the application icon is unavailable for the tray")?;

    let suppress_release = AtomicBool::new(false);
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            SETTINGS_ID => open_settings(app, "bar"),
            ABOUT_ID => open_settings(app, "info"),
            AUTOSTART_ID => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    match autostart::toggle(app.clone()).await {
                        Ok(AutostartStatus::Ready { .. }) => {}
                        Ok(_) => open_settings(&app, "system"),
                        Err(error) => {
                            tracing::error!(%error, "tray autostart failed; opening Settings > System");
                            open_settings(&app, "system");
                        }
                    }
                });
            }
            RESTART_ID => {
                release_platform_state();
                app.request_restart();
            }
            QUIT_ID => {
                release_platform_state();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            match click_action(event, &suppress_release) {
                Some(ClickAction::ShowBar) => show_and_focus(tray.app_handle()),
                Some(ClickAction::OpenSettings) => open_settings(tray.app_handle(), "bar"),
                None => {}
            }
        })
        .build(app)
        .context("failed to build the tray icon")?;
    app.listen("autostart-changed", move |event| {
        let result = (|| -> anyhow::Result<()> {
            let status: AutostartStatus = serde_json::from_str(event.payload())?;
            autostart_item.set_checked(status.registered().unwrap_or(false))?;
            autostart_item.set_enabled(status.registered().is_some())?;
            Ok(())
        })();
        if let Err(error) = result {
            tracing::warn!(%error, "could not refresh tray autostart; use Settings > System");
        }
    });
    // Linux trays do not report menu-open events. Refresh external opt-outs
    // without writing the OS setting; our own changes are emitted immediately.
    if !matches!(status, AutostartStatus::Unavailable) {
        let app = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(error) = autostart::get_autostart_status(app.clone()).await {
                    tracing::warn!(%error, "could not refresh tray autostart; use Settings > System");
                }
            }
        });
    }
    Ok(())
}

/// Releases the reserved screen edge before a quit or restart that the user
/// asked for (tray menu, declined terms).
pub(crate) fn release_platform_state() {
    if let Err(error) = crate::platform::shutdown() {
        tracing::error!(%error, "failed to release native platform state before quitting");
    }
}

fn open_settings(app: &AppHandle, section: &'static str) {
    let app = app.clone();
    // Creating WebView2 from a synchronous event handler can deadlock Windows.
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::surfaces::open_settings_from_app(&app, section) {
            tracing::warn!(%error, %section, "failed to open settings from the tray");
        }
    });
}

fn show_and_focus(app: &AppHandle) {
    let Some(window) = app.get_webview_window("bar") else {
        tracing::warn!("cannot show the bar from the tray because window `bar` is missing");
        return;
    };
    if let Err(error) = window.unminimize() {
        tracing::warn!(%error, "failed to unminimize the bar from the tray");
    }
    if let Err(error) = crate::platform::show_surface(&window) {
        tracing::warn!(%error, "failed to show the bar from the tray");
    }
    if let Err(error) = crate::platform::window::focus(&window) {
        tracing::warn!(%error, "failed to focus the bar from the tray");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(button_state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click {
            id: "test".into(),
            position: tauri::PhysicalPosition::new(0.0, 0.0),
            rect: tauri::Rect {
                position: tauri::PhysicalPosition::new(0, 0).into(),
                size: tauri::PhysicalSize::new(16, 16).into(),
            },
            button: MouseButton::Left,
            button_state,
        }
    }

    #[test]
    fn double_click_opens_settings_without_the_final_release_stealing_focus() {
        let state = AtomicBool::new(false);
        assert_eq!(click_action(click(MouseButtonState::Down), &state), None);
        assert_eq!(
            click_action(click(MouseButtonState::Up), &state),
            Some(ClickAction::ShowBar)
        );
        let TrayIconEvent::Click {
            id, position, rect, ..
        } = click(MouseButtonState::Down)
        else {
            unreachable!()
        };
        assert_eq!(
            click_action(
                TrayIconEvent::DoubleClick {
                    id,
                    position,
                    rect,
                    button: MouseButton::Left
                },
                &state
            ),
            Some(ClickAction::OpenSettings)
        );
        assert_eq!(click_action(click(MouseButtonState::Up), &state), None);
        assert_eq!(click_action(click(MouseButtonState::Down), &state), None);
        assert_eq!(
            click_action(click(MouseButtonState::Up), &state),
            Some(ClickAction::ShowBar)
        );
    }
}
