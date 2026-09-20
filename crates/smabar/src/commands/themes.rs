//! Theme save/delete/export/import commands. All document I/O and
//! validation live in [`smabar_core::themes::io`], shared with the MCP
//! `theme_write` tool; these commands only add config plumbing and events.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use smabar_core::config::{ConfigWatcher, SmabarConfig, SmabarPaths, update as config_update};
use smabar_core::themes::{self, io as theme_io};
use tauri::{AppHandle, State};

use super::AppState;
use super::config::emit_theme_changed;

/// Expands a leading `~` and requires an absolute path — commands never
/// resolve paths relative to the app's working directory.
fn expand_user_path(state: &AppState, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("path is empty".to_string());
    }
    let home = || {
        state
            .paths
            .base_dir()
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| "cannot resolve the home directory".to_string())
    };
    let path = if trimmed == "~" {
        home()?
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        home()?.join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    if !path.is_absolute() {
        return Err(format!(
            "\"{trimmed}\" is not an absolute path (start it with / or ~/)"
        ));
    }
    Ok(path)
}

/// The export target: the configured `themeExportDir` or the default
/// `themes/export` when it is empty.
fn export_dir(state: &AppState, configured: &str) -> Result<PathBuf, String> {
    if configured.trim().is_empty() {
        Ok(state.paths.themes_dir().join("export"))
    } else {
        expand_user_path(state, configured).map_err(|error| format!("themeExportDir: {error}"))
    }
}

/// Computes the same activation as `update_config("theme", …)` against the
/// snapshot held by ConfigWatcher's atomic update.
fn activated_config(
    paths: &SmabarPaths,
    current: &SmabarConfig,
    name: &str,
) -> Result<SmabarConfig, String> {
    let (updated, _theme_warnings) =
        config_update::set_config_path_activating(paths, current, "theme", json!(name))
            .map_err(|error| error.to_string())?;
    Ok(updated)
}

fn fresh_summaries(state: &AppState) -> Vec<themes::ThemeInfo> {
    themes::summaries(&state.paths, &state.watcher.current())
}

pub(super) fn emit_theme_if_active(
    app: &AppHandle,
    paths: &SmabarPaths,
    watcher: &ConfigWatcher,
    name: &str,
) {
    watcher.with_current(|current| {
        if current.theme == name {
            emit_theme_changed(app, paths, name);
        }
    });
}

fn switch_away_if_active(
    paths: &SmabarPaths,
    watcher: &ConfigWatcher,
    name: &str,
) -> Result<Option<(SmabarConfig, SmabarConfig)>, String> {
    watcher
        .update(|current| {
            if current.theme != name {
                return (current.clone(), Ok(None));
            }
            match activated_config(paths, current, "default") {
                Ok(updated) => (updated.clone(), Ok(Some((current.clone(), updated)))),
                Err(error) => (current.clone(), Err(error)),
            }
        })
        .map_err(|error| error.to_string())?
}

fn rollback_theme_write(pending: theme_io::PendingThemeWrite, error: String) -> String {
    match pending.rollback() {
        Ok(true) => error,
        Ok(false) => {
            format!("{error}; the theme file changed concurrently, so its newer contents were kept")
        }
        Err(rollback) => {
            format!("{error}; additionally failed to restore the theme file: {rollback}")
        }
    }
}

fn rollback_theme_write_safely(
    watcher: &ConfigWatcher,
    name: &str,
    was_active: bool,
    pending: theme_io::PendingThemeWrite,
    error: String,
) -> String {
    let rollback = watcher.update(|current| {
        if !was_active && current.theme == name {
            return (current.clone(), None);
        }
        (current.clone(), Some(pending.rollback()))
    });
    match rollback {
        Ok(Some(Ok(true))) => error,
        Ok(Some(Ok(false))) => {
            format!("{error}; the theme file changed concurrently, so its newer contents were kept")
        }
        Ok(Some(Err(rollback))) => {
            format!("{error}; additionally failed to restore the theme file: {rollback}")
        }
        Ok(None) => format!("{error}; the theme was activated concurrently, so its file was kept"),
        Err(rollback) => {
            format!("{error}; additionally failed to serialize the rollback: {rollback}")
        }
    }
}

pub(super) fn save_theme_file(
    paths: &SmabarPaths,
    watcher: &ConfigWatcher,
    name: &str,
    overwrite: bool,
) -> Result<(bool, String), String> {
    let mut pending = None;
    let updated = watcher.update(|current| {
        let document = theme_io::current_look_document(paths, current);
        let json = match theme_io::document_to_json(&document) {
            Ok(json) => json,
            Err(error) => return (current.clone(), Err(error.to_string())),
        };
        let staged = match theme_io::stage_theme_write(paths, name, &document, overwrite) {
            Ok(staged) => staged,
            Err(error) => return (current.clone(), Err(error.to_string())),
        };
        match activated_config(paths, current, name) {
            Ok(updated) => {
                pending = Some((current.theme == name, staged));
                (updated, Ok((current.theme == name, json)))
            }
            Err(error) => (current.clone(), Err(rollback_theme_write(staged, error))),
        }
    });
    match updated {
        Ok(result) => result,
        Err(error) => {
            let error = error.to_string();
            Err(match pending {
                Some((was_active, pending)) => {
                    rollback_theme_write_safely(watcher, name, was_active, pending, error)
                }
                None => error,
            })
        }
    }
}

/// Saves the current look (active theme + slider overrides + live behavior
/// settings) as the drop-in `themes/<name>.json` and activates it — the bar
/// keeps looking identical while the overrides move into the theme file.
/// Returns the fresh theme list.
#[tauri::command(async)]
pub fn save_custom_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    overwrite: bool,
) -> Result<Vec<themes::ThemeInfo>, String> {
    if save_theme_file(&state.paths, &state.watcher, &name, overwrite)?.0 {
        // Re-save under the already active name: the event bridge compares
        // names only and stays silent, while the activation just cleared the
        // overrides this file baked in — emit the resolved map manually.
        emit_theme_if_active(&app, &state.paths, &state.watcher, &name);
    }
    Ok(fresh_summaries(&state))
}

fn delete_theme_file(
    paths: &SmabarPaths,
    watcher: &ConfigWatcher,
    name: &str,
) -> Result<(), String> {
    let activation = switch_away_if_active(paths, watcher, name)?;
    if let Err(error) = theme_io::delete_theme(paths, name) {
        if let Some((original, activated)) = activation {
            let restored = watcher
                .update(|current| {
                    if *current == activated {
                        (original, true)
                    } else {
                        (current.clone(), false)
                    }
                })
                .map_err(|rollback| {
                    format!("{error}; additionally failed to restore the active theme: {rollback}")
                })?;
            if !restored {
                return Err(format!(
                    "{error}; the config changed concurrently, so the active theme was left on default instead of overwriting that newer change"
                ));
            }
        }
        return Err(error.to_string());
    }
    // An activation may have won the config lock after the first switch but
    // before deletion. Reconcile once more after the file is gone; future
    // activations are rejected by the shared availability check.
    switch_away_if_active(paths, watcher, name)?;
    Ok(())
}

/// Deletes the drop-in `themes/<name>.json`. When it is the active theme the
/// command switches to `default` and restores the previous config if deletion
/// fails without a concurrent config change. Returns the fresh theme list.
#[tauri::command(async)]
pub fn delete_theme(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<themes::ThemeInfo>, String> {
    delete_theme_file(&state.paths, &state.watcher, &name)?;
    state.store().note_removed();
    Ok(fresh_summaries(&state))
}

/// Exports theme `name` as a self-contained file into the export directory
/// (see [`export_dir`]); collisions get a numeric suffix. Returns the
/// absolute path of the written file.
#[tauri::command(async)]
pub fn export_theme(
    state: State<'_, AppState>,
    name: String,
    directory: Option<String>,
) -> Result<String, String> {
    let configured = directory.unwrap_or_else(|| state.watcher.current().theme_export_dir);
    let dir = export_dir(&state, &configured)?;
    let target =
        theme_io::export_to_dir(&state.paths, &name, &dir).map_err(|error| error.to_string())?;
    Ok(target.display().to_string())
}

/// Imports one theme file (from the path input or a drag-and-drop) into the
/// drop-in directory; the theme name is the slugified file stem. Returns the
/// fresh theme list — the imported theme appears but is NOT activated.
#[tauri::command(async)]
pub fn import_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    overwrite: bool,
) -> Result<Vec<themes::ThemeInfo>, String> {
    let file = expand_user_path(&state, &path)?;
    let name = theme_io::import_theme_file(&state.paths, &file, overwrite)
        .map_err(|error| error.to_string())?;
    // Overwriting the active theme needs a repaint because the config name
    // did not change. The lock keeps this event ordered with activation.
    emit_theme_if_active(&app, &state.paths, &state.watcher, &name);
    Ok(fresh_summaries(&state))
}

/// What the export UI shows: the raw configured value (editable) and the
/// directory exports actually land in right now.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportDirInfo {
    configured: String,
    effective: Option<String>,
    error: Option<String>,
}

#[tauri::command]
pub fn get_theme_export_dir(state: State<'_, AppState>) -> Result<ExportDirInfo, String> {
    let config = state.watcher.current();
    let (effective, error) = match export_dir(&state, &config.theme_export_dir) {
        Ok(path) => (Some(path.display().to_string()), None),
        Err(error) => (None, Some(error)),
    };
    Ok(ExportDirInfo {
        configured: config.theme_export_dir,
        effective,
        error,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn saved_copy_uses_the_exact_local_snapshot_even_after_later_edits() {
        let dir = tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let watcher = ConfigWatcher::spawn(paths.clone()).expect("watcher");
        let mut config = watcher.current();
        config
            .appearance
            .tokens
            .insert("--sb-accent".into(), "#123456".into());
        watcher.apply(config).expect("edit");
        let (_, document) = save_theme_file(&paths, &watcher, "mine", false).expect("save");
        assert_eq!(
            std::fs::read_to_string(paths.themes_dir().join("mine.json")).expect("local copy"),
            document
        );
        let mut later = watcher.current();
        later
            .appearance
            .tokens
            .insert("--sb-accent".into(), "#abcdef".into());
        watcher.apply(later).expect("later edit");
        let output = dir.path().join("shared.json");
        theme_io::write_copy(&output, &document).expect("retry copy");
        assert_eq!(
            std::fs::read_to_string(&output).expect("external copy"),
            document
        );
        assert!(theme_io::write_copy(&output, "{}").is_err());
        assert_eq!(
            std::fs::read_to_string(output).expect("preserved copy"),
            document
        );
    }

    #[tokio::test]
    async fn failed_active_theme_delete_restores_the_exact_config() {
        let dir = tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let watcher = ConfigWatcher::spawn(paths.clone()).expect("watcher");
        let mut active = watcher.current();
        active.theme = "mine".to_string();
        active.theme_export_dir = "/keep-this-value".to_string();
        watcher.apply(active.clone()).expect("activate test theme");

        std::fs::create_dir_all(paths.themes_dir().join("mine.json"))
            .expect("directory makes remove_file fail deterministically");
        let error = delete_theme_file(&paths, &watcher, "mine").expect_err("delete failure");

        assert!(error.contains("cannot delete"));
        assert_eq!(watcher.current(), active);
    }

    #[tokio::test]
    async fn late_activation_is_reconciled_after_the_theme_file_is_deleted() {
        let dir = tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let watcher = ConfigWatcher::spawn(paths.clone()).expect("watcher");
        std::fs::create_dir_all(paths.themes_dir()).expect("themes dir");
        std::fs::write(paths.themes_dir().join("mine.json"), "{}\n").expect("theme");
        theme_io::delete_theme(&paths, "mine").expect("delete");

        let mut raced = watcher.current();
        raced.theme = "mine".to_string();
        watcher.apply(raced).expect("simulate in-flight activation");
        switch_away_if_active(&paths, &watcher, "mine").expect("reconcile");

        assert_eq!(watcher.current().theme, "default");
    }

    #[tokio::test]
    async fn failed_config_persist_rolls_back_overwrite_and_new_theme_file() {
        let dir = tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let watcher = ConfigWatcher::spawn(paths.clone()).expect("watcher");
        let old =
            theme_io::parse_document_strict(r##"{"--sb-accent":"#123456"}"##).expect("old theme");
        theme_io::write_theme(&paths, "mine", &old, false).expect("write old theme");
        let file = paths.themes_dir().join("mine.json");
        let before = std::fs::read(&file).expect("read old theme");

        std::fs::remove_file(paths.config_file()).expect("remove config file");
        std::fs::create_dir(paths.config_file()).expect("block config replacement");

        save_theme_file(&paths, &watcher, "mine", true).expect_err("config write must fail");
        assert_eq!(std::fs::read(&file).expect("restored theme"), before);

        save_theme_file(&paths, &watcher, "fresh", false).expect_err("config write must fail");
        assert!(!paths.themes_dir().join("fresh.json").exists());
    }

    #[tokio::test]
    async fn failed_save_keeps_a_theme_activated_by_a_concurrent_update() {
        let dir = tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let watcher = ConfigWatcher::spawn(paths.clone()).expect("watcher");
        let pending = theme_io::stage_current_theme(&paths, &watcher.current(), "mine", false)
            .expect("stage theme");
        let mut activated = watcher.current();
        activated.theme = "mine".to_string();
        watcher.apply(activated).expect("concurrent activation");

        let error = rollback_theme_write_safely(
            &watcher,
            "mine",
            false,
            pending,
            "config write failed".to_string(),
        );

        assert!(error.contains("activated concurrently"));
        assert!(paths.themes_dir().join("mine.json").is_file());
    }
}
