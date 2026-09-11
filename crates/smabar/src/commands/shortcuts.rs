//! Commands for the pinned-shortcut zone: app search, icons, pin
//! management, and launching. All mutations run through
//! `ConfigWatcher::apply`, so MCP clients and the config file stay in sync.

use std::path::PathBuf;

use serde::Serialize;
use smabar_core::config::{LabelMode, ShortcutEntry, ShortcutsConfig, SpecialShortcut};
use smabar_core::shortcuts::{self, AppSource, ResolvedShortcut, ShortcutsService};
use tauri::State;

use super::AppState;

/// The shortcut zone as the shell renders it: resolved pins plus the
/// display options that live in the `shortcuts` config section.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutsUiState {
    pinned: Vec<ResolvedShortcut>,
    /// Label placement (right | below | hidden).
    labels: LabelMode,
    /// Unclamped config values — the shell clamps (16–64 / 9–16).
    icon_size: u32,
    label_size: u32,
    /// Raw config entries, index-aligned with `pinned` — the settings panel
    /// reorders these and writes the array back via
    /// `update_config shortcuts.pinned`.
    entries: Vec<ShortcutEntry>,
}

impl ShortcutsUiState {
    pub fn resolve(service: &ShortcutsService, config: &ShortcutsConfig) -> Self {
        Self {
            pinned: service.resolve_pinned(config),
            labels: config.labels,
            icon_size: config.icon_size,
            label_size: config.label_size,
            entries: config.pinned.clone(),
        }
    }
}

/// One installed application for the settings search. Exactly one source is
/// set: `desktopId` on Linux or `path` on Windows. Icons load lazily through
/// `get_app_icon`; inlining every result would be megabytes.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    desktop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

/// Installed applications, filtered by an optional case-insensitive query
/// over name and comment (no query lists all).
#[tauri::command(async)]
pub fn list_apps(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<AppEntry>, String> {
    Ok(state
        .shortcuts
        .search(query.as_deref().unwrap_or(""))
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|app| {
            let (desktop_id, path) = match app.source {
                AppSource::DesktopId(id) => (Some(id), None),
                AppSource::Path(path) => (None, Some(path)),
            };
            AppEntry {
                desktop_id,
                path,
                name: app.name,
                comment: app.comment,
            }
        })
        .collect())
}

/// Icon of an installed application as a data URI (`None` when unresolved —
/// the shell shows an initial-letter tile then). Exactly one source is required.
#[tauri::command(async)]
pub fn get_app_icon(
    state: State<'_, AppState>,
    desktop_id: Option<String>,
    path: Option<String>,
) -> Result<Option<String>, String> {
    let path = path.map(PathBuf::from);
    state
        .shortcuts
        .app_icon_data_uri_for_source(desktop_id.as_deref(), path.as_deref())
        .map_err(|error| error.to_string())
}

/// The shortcut zone state: pinned shortcuts resolved for display (labels +
/// inline icons) plus the display options. The shell refetches this on the
/// `shortcuts-changed` event (icons are too heavy to broadcast).
#[tauri::command(async)]
pub fn get_shortcuts(state: State<'_, AppState>) -> ShortcutsUiState {
    let config = state.watcher.current();
    ShortcutsUiState::resolve(&state.shortcuts, &config.shortcuts)
}

/// Pins an application/file/folder, website, or visual separator at `index`
/// (appends when omitted).
#[tauri::command(async)]
pub fn pin_shortcut(
    state: State<'_, AppState>,
    desktop_id: Option<String>,
    path: Option<String>,
    url: Option<String>,
    label: Option<String>,
    index: Option<usize>,
    separator: Option<bool>,
) -> Result<(), String> {
    let entry = state
        .shortcuts
        .validated_entry(
            desktop_id,
            path.map(PathBuf::from),
            url,
            label,
            separator.unwrap_or(false),
        )
        .map_err(|error| error.to_string())?;
    apply_pin(&state, entry, index)
}

/// Appends one curated native system item.
#[tauri::command]
pub fn pin_special_shortcut(
    state: State<'_, AppState>,
    special: SpecialShortcut,
) -> Result<(), String> {
    let entry = state
        .shortcuts
        .validated_entry_with_special(None, None, None, Some(special), None, false)
        .map_err(|error| error.to_string())?;
    apply_pin(&state, entry, None)
}

fn apply_pin(state: &AppState, entry: ShortcutEntry, index: Option<usize>) -> Result<(), String> {
    let result = state
        .watcher
        .update(
            |current| match shortcuts::pin_shortcut(current, entry, index) {
                Ok(new) => (new, Ok(())),
                Err(error) => (current.clone(), Err(error)),
            },
        )
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

/// Removes a pinned shortcut by its pin id.
#[tauri::command]
pub fn unpin_shortcut(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let result = state
        .watcher
        .update(|current| match shortcuts::unpin_shortcut(current, &id) {
            Ok(new) => (new, Ok(())),
            Err(error) => (current.clone(), Err(error)),
        })
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

/// Launches a pinned shortcut as a detached process (survives the bar).
#[tauri::command(async)]
pub fn launch_shortcut(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let config = state.watcher.current();
    let entry = config
        .shortcuts
        .pinned
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| format!("no pinned shortcut with id \"{id}\""))?;
    state
        .shortcuts
        .launch_pinned(entry)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell consumes this shape verbatim (camelCase, `entries` carrying
    /// the raw config pins for reorder round-trips through `update_config`).
    #[test]
    fn shortcuts_ui_state_serializes_entries_camel_case() {
        let state = ShortcutsUiState {
            pinned: vec![
                ResolvedShortcut {
                    id: "sc-1".into(),
                    label: "Files".into(),
                    icons: Vec::new(),
                    desktop_id: Some("org.gnome.Nautilus.desktop".into()),
                    separator: false,
                },
                ResolvedShortcut {
                    id: "sc-separator".into(),
                    label: String::new(),
                    icons: Vec::new(),
                    desktop_id: None,
                    separator: true,
                },
            ],
            labels: LabelMode::Below,
            icon_size: 40,
            label_size: 11,
            entries: vec![
                ShortcutEntry {
                    id: "sc-1".into(),
                    desktop_id: Some("org.gnome.Nautilus.desktop".into()),
                    ..ShortcutEntry::default()
                },
                ShortcutEntry {
                    id: "sc-separator".into(),
                    separator: true,
                    ..ShortcutEntry::default()
                },
            ],
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["labels"], "below");
        assert_eq!(json["iconSize"], 40);
        assert_eq!(json["labelSize"], 11);
        assert_eq!(json["pinned"][0]["label"], "Files");
        assert_eq!(json["pinned"][0]["separator"], false);
        assert_eq!(json["pinned"][1]["separator"], true);
        let entry = &json["entries"][0];
        assert_eq!(entry["id"], "sc-1");
        assert_eq!(entry["desktopId"], "org.gnome.Nautilus.desktop");
        // Unset options are omitted so the array round-trips through
        // `shortcuts.pinned` without spurious nulls.
        assert!(entry.get("path").is_none());
        assert!(entry.get("url").is_none());
        assert!(entry.get("special").is_none());
        assert!(entry.get("label").is_none());
        assert!(entry.get("separator").is_none());
        assert_eq!(json["entries"][1]["separator"], true);
    }

    #[test]
    fn app_entries_serialize_exactly_one_platform_source() {
        let desktop = serde_json::to_value(AppEntry {
            desktop_id: Some("firefox.desktop".into()),
            path: None,
            name: "Firefox".into(),
            comment: None,
        })
        .expect("serialize desktop app");
        assert_eq!(desktop["desktopId"], "firefox.desktop");
        assert!(desktop.get("path").is_none());

        let native = serde_json::to_value(AppEntry {
            desktop_id: None,
            path: Some(PathBuf::from(r"C:\ProgramData\Blender.lnk")),
            name: "Blender".into(),
            comment: None,
        })
        .expect("serialize native app");
        assert!(native.get("desktopId").is_none());
        assert_eq!(native["path"], r"C:\ProgramData\Blender.lnk");
    }
}
