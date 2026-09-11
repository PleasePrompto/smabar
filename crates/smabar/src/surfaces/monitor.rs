//! One monitor decision shared by every native smabar surface.

use anyhow::Context;
use serde::Serialize;
use smabar_core::config::MonitorPreference;
use smabar_core::platform::surfaces::ScreenRect;
use tauri::{AppHandle, Emitter, Manager, State};

use super::SurfaceManager;
use crate::commands::AppState;
use crate::platform::display::{self, DisplayOption, DisplaySnapshot};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorState {
    monitors: Vec<DisplayOption>,
    effective_id: String,
    preferred_connected: bool,
}

impl SurfaceManager {
    pub(crate) fn initialize_monitors(&self, app: &AppHandle) -> anyhow::Result<()> {
        let displays = display::detect(app)?;
        let preference = app.state::<AppState>().config().layout.monitor;
        let index = resolve_index(&displays, preference.as_ref())
            .context("no connected monitor is available")?;
        let effective = displays[index].clone();
        let fallback =
            fallback_reason(&displays, preference.as_ref(), &effective).unwrap_or("none");
        let connected = displays.len();
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        lifecycle.bar_work_area = Some(effective.work_area);
        lifecycle.effective_monitor_id = Some(effective.id.clone());
        lifecycle.displays = displays;
        tracing::info!(
            preferred = preference.as_ref().map(|value| value.id.as_str()),
            effective = effective.id,
            fallback_reason = fallback,
            connected,
            "monitor selection initialized"
        );
        Ok(())
    }

    pub(crate) fn active_monitor(&self) -> anyhow::Result<DisplaySnapshot> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let id = lifecycle
            .effective_monitor_id
            .as_deref()
            .context("monitor selection is not initialized")?;
        lifecycle
            .displays
            .iter()
            .find(|display| display.id == id)
            .cloned()
            .context("effective monitor is no longer connected")
    }

    pub(crate) async fn reconcile_monitors(
        &self,
        app: &AppHandle,
        detected: Option<Vec<DisplaySnapshot>>,
        reason: &'static str,
    ) -> anyhow::Result<bool> {
        let mut displays = match detected {
            Some(displays) => displays,
            None => self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?
                .displays
                .clone(),
        };
        self.preserve_effective_work_area(&mut displays)?;
        let preference = app.state::<AppState>().config().layout.monitor;
        let index = resolve_index(&displays, preference.as_ref())
            .context("no connected monitor is available")?;
        let effective = displays[index].clone();
        let fallback =
            fallback_reason(&displays, preference.as_ref(), &effective).unwrap_or("none");
        let connected = displays.len();
        let (previous, inventory_changed) = {
            let lifecycle = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
            let previous = lifecycle.effective_monitor_id.as_ref().and_then(|id| {
                lifecycle
                    .displays
                    .iter()
                    .find(|display| &display.id == id)
                    .cloned()
            });
            (previous, lifecycle.displays != displays)
        };
        let target_changed = previous
            .as_ref()
            .is_none_or(|previous| !same_target(previous, &effective));
        if !target_changed {
            self.store_inventory(displays, &effective)?;
            if inventory_changed || reason == "preference" {
                app.emit("monitors-changed", ())
                    .context("failed to report monitor inventory change")?;
            }
            if inventory_changed {
                tracing::info!(
                    reason,
                    preferred = preference.as_ref().map(|value| value.id.as_str()),
                    effective = effective.id,
                    fallback_reason = fallback,
                    connected,
                    "monitor topology changed without relocating surfaces"
                );
            }
            return Ok(false);
        }

        let bar = app
            .get_webview_window("bar")
            .context("bar surface is unavailable during monitor change")?;
        let settings = app.get_webview_window(super::SurfaceRole::Settings.label());
        let settings_was_visible = settings
            .as_ref()
            .map(|window| window.is_visible())
            .transpose()
            .context("failed to read settings visibility before monitor change")?
            .unwrap_or(false);
        let notifications = app.get_webview_window(super::SurfaceRole::Notifications.label());
        let notifications_were_visible = notifications
            .as_ref()
            .map(|window| window.is_visible())
            .transpose()
            .context("failed to read notification visibility before monitor change")?
            .unwrap_or(false);
        let concealed = crate::platform::set_bar_transition_opaque(&bar, false)
            .await
            .context("failed to conceal bar before monitor change")?;
        let move_result = async {
            if settings_was_visible && let Some(window) = settings.as_ref() {
                window
                    .hide()
                    .context("failed to conceal settings before monitor change")?;
            }
            if notifications_were_visible && let Some(window) = notifications.as_ref() {
                crate::platform::stage_transient_update(window)
                    .await
                    .context("failed to conceal notifications before monitor change")?;
            }
            let position = app.state::<AppState>().config().layout.position;
            self.prepare_bar_relocation(app, position).await?;
            crate::platform::strut::apply(&bar, None, None)
                .context("failed to release the previous monitor reservation")?;
            self.store_inventory(displays, &effective)?;
            for role in [
                super::SurfaceRole::Bar,
                super::SurfaceRole::Overlay,
                super::SurfaceRole::Settings,
                super::SurfaceRole::Notifications,
            ] {
                if let Some(window) = app.get_webview_window(role.label()) {
                    crate::platform::window::set_surface_monitor(&window, &effective)?;
                }
            }
            if let Some(settings) = settings.as_ref() {
                let geometry = app.state::<AppState>().config().settings_window;
                crate::platform::window::place_settings_surface(settings, &effective, geometry)?;
                if settings_was_visible {
                    settings
                        .show()
                        .context("failed to reveal settings after monitor change")?;
                    settings
                        .set_focus()
                        .context("failed to refocus settings after monitor change")?;
                }
            }
            app.emit("surface-context-changed", ())
                .context("failed to refresh surface work-area context")?;
            app.emit("monitors-changed", ())
                .context("failed to report effective monitor change")?;
            tracing::info!(
                reason,
                preferred = preference.as_ref().map(|value| value.id.as_str()),
                previous = previous.as_ref().map(|value| value.id.as_str()),
                effective = effective.id,
                fallback_reason = fallback,
                connected,
                "effective monitor changed"
            );
            Ok(())
        }
        .await;
        if move_result.is_err() {
            if concealed {
                let _ = crate::platform::set_bar_transition_opaque(&bar, true).await;
            }
            if settings_was_visible && let Some(window) = settings {
                let _ = window.show();
            }
            if notifications_were_visible
                && let Some(window) = notifications
                && let Ok(token) = crate::platform::transient_presentation_token(&window)
            {
                let _ = crate::platform::present_transient(&window, token);
            }
        }
        move_result.map(|()| true)
    }

    fn store_inventory(
        &self,
        displays: Vec<DisplaySnapshot>,
        effective: &DisplaySnapshot,
    ) -> anyhow::Result<()> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        lifecycle.displays = displays;
        lifecycle.effective_monitor_id = Some(effective.id.clone());
        lifecycle.bar_work_area = Some(effective.work_area);
        Ok(())
    }

    fn preserve_effective_work_area(&self, displays: &mut [DisplaySnapshot]) -> anyhow::Result<()> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let Some(previous) = lifecycle
            .effective_monitor_id
            .as_ref()
            .and_then(|id| lifecycle.displays.iter().find(|display| &display.id == id))
        else {
            return Ok(());
        };
        if let Some(current) = displays
            .iter_mut()
            .find(|display| display.id == previous.id)
        {
            current.work_area = project_work_area(previous, current.frame);
        }
        Ok(())
    }

    fn monitor_state(
        &self,
        preference: Option<&MonitorPreference>,
    ) -> anyhow::Result<MonitorState> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let effective_id = lifecycle
            .effective_monitor_id
            .clone()
            .context("monitor selection is not initialized")?;
        Ok(MonitorState {
            monitors: display::options(&lifecycle.displays),
            effective_id,
            preferred_connected: preference.is_none_or(|preferred| {
                lifecycle
                    .displays
                    .iter()
                    .any(|display| display.id == preferred.id)
            }),
        })
    }
}

fn resolve_index(
    displays: &[DisplaySnapshot],
    preference: Option<&MonitorPreference>,
) -> Option<usize> {
    preference
        .and_then(|preferred| displays.iter().position(|item| item.id == preferred.id))
        .or_else(|| displays.iter().position(|item| item.primary))
        .or((!displays.is_empty()).then_some(0))
}

fn same_target(left: &DisplaySnapshot, right: &DisplaySnapshot) -> bool {
    left.id == right.id
        && left.frame == right.frame
        && left.work_area == right.work_area
        && left.scale_factor == right.scale_factor
}

fn fallback_reason(
    displays: &[DisplaySnapshot],
    preference: Option<&MonitorPreference>,
    effective: &DisplaySnapshot,
) -> Option<&'static str> {
    match preference {
        Some(preferred) if preferred.id != effective.id => Some(if effective.primary {
            "preferred-disconnected-primary"
        } else {
            "preferred-disconnected-first"
        }),
        None if !effective.primary && !displays.is_empty() => Some("primary-unavailable-first"),
        _ => None,
    }
}

fn project_work_area(previous: &DisplaySnapshot, frame: ScreenRect) -> ScreenRect {
    let horizontal = insets(
        previous.frame.x,
        previous.frame.w,
        previous.work_area.x,
        previous.work_area.w,
    );
    let vertical = insets(
        previous.frame.y,
        previous.frame.h,
        previous.work_area.y,
        previous.work_area.h,
    );
    let left = horizontal.0.min(frame.w.saturating_sub(1));
    let right = horizontal.1.min(frame.w.saturating_sub(left + 1));
    let top = vertical.0.min(frame.h.saturating_sub(1));
    let bottom = vertical.1.min(frame.h.saturating_sub(top + 1));
    ScreenRect {
        x: frame
            .x
            .saturating_add(i32::try_from(left).unwrap_or(i32::MAX)),
        y: frame
            .y
            .saturating_add(i32::try_from(top).unwrap_or(i32::MAX)),
        w: frame.w - left - right,
        h: frame.h - top - bottom,
    }
}

fn insets(frame_start: i32, frame_size: u32, work_start: i32, work_size: u32) -> (u32, u32) {
    let frame_end = i64::from(frame_start) + i64::from(frame_size);
    let work_end = i64::from(work_start) + i64::from(work_size);
    (
        u32::try_from((i64::from(work_start) - i64::from(frame_start)).max(0)).unwrap_or(u32::MAX),
        u32::try_from((frame_end - work_end).max(0)).unwrap_or(u32::MAX),
    )
}

#[tauri::command]
pub fn get_monitor_state(
    app: AppHandle,
    manager: State<'_, SurfaceManager>,
) -> Result<MonitorState, String> {
    let preference = app.state::<AppState>().config().layout.monitor;
    manager
        .monitor_state(preference.as_ref())
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: &str, primary: bool) -> DisplaySnapshot {
        DisplaySnapshot {
            id: id.to_string(),
            label: id.to_string(),
            frame: ScreenRect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            work_area: ScreenRect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            },
            scale_factor: 1.0,
            primary,
        }
    }

    #[test]
    fn resolves_preference_then_primary_then_first() {
        let displays = vec![display("left", false), display("right", true)];
        let selected = MonitorPreference {
            id: "left".to_string(),
            label: "Left".to_string(),
        };
        let missing = MonitorPreference {
            id: "gone".to_string(),
            label: "Gone".to_string(),
        };
        assert_eq!(resolve_index(&displays, Some(&selected)), Some(0));
        assert_eq!(resolve_index(&displays, Some(&missing)), Some(1));
        assert_eq!(
            fallback_reason(&displays, Some(&missing), &displays[1]),
            Some("preferred-disconnected-primary")
        );
        assert_eq!(resolve_index(&displays, None), Some(1));
        let only = [display("only", false)];
        assert_eq!(resolve_index(&only, None), Some(0));
        assert_eq!(
            fallback_reason(&only, None, &only[0]),
            Some("primary-unavailable-first")
        );
        assert_eq!(resolve_index(&[], None), None);

        let reconnected = vec![display("left", true), display("gone", false)];
        assert_eq!(resolve_index(&reconnected, Some(&missing)), Some(1));
        let new_primary = vec![display("left", true), display("right", false)];
        assert_eq!(resolve_index(&new_primary, None), Some(0));
    }

    #[test]
    fn relocates_only_when_the_effective_target_geometry_changes() {
        let current = display("right", true);
        let mut metadata_only = current.clone();
        metadata_only.primary = false;
        metadata_only.label = "Renamed".to_string();
        assert!(same_target(&current, &metadata_only));

        let mut moved = current.clone();
        moved.frame.x = 1920;
        assert!(!same_target(&current, &moved));
    }

    #[test]
    fn topology_refresh_does_not_count_smabars_own_reservation_twice() {
        let mut previous = display("desk", true);
        previous.work_area.y = 40;
        previous.work_area.h = 1_040;
        let projected = project_work_area(
            &previous,
            ScreenRect {
                x: 1_920,
                y: 100,
                w: 2_560,
                h: 1_440,
            },
        );
        assert_eq!(
            projected,
            ScreenRect {
                x: 1_920,
                y: 140,
                w: 2_560,
                h: 1_400,
            }
        );
    }
}
