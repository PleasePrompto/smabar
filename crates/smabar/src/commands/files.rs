//! Native file selection and theme copies, available only to Settings.
use super::AppState;
use serde::{Deserialize, Serialize};
use smabar_core::themes::{self, io as theme_io};
use std::path::PathBuf;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilePurpose {
    ThemeImport,
    ThemeSave,
    PluginZip,
}

#[tauri::command]
pub async fn choose_settings_file(
    window: WebviewWindow,
    purpose: FilePurpose,
    suggested: Option<String>,
) -> Result<Option<String>, String> {
    let save = matches!(purpose, FilePurpose::ThemeSave);
    let (label, extension) = if matches!(purpose, FilePurpose::PluginZip) {
        ("Plugin ZIP", "zip")
    } else {
        ("smabar Theme", "json")
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter(label, &[extension]);
    if let Some(name) = suggested {
        dialog = dialog.set_file_name(name);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let callback = move |file: Option<tauri_plugin_dialog::FilePath>| {
        if sender.send(file).is_err() {
            tracing::debug!("file dialog caller closed");
        }
    };
    if save {
        dialog.save_file(callback);
    } else {
        dialog.pick_file(callback);
    }
    receiver
        .await
        .map_err(|error| error.to_string())?
        .map(|path| {
            path.into_path()
                .map(|mut path| {
                    if save && path.extension().is_none() {
                        path.set_extension("json");
                    }
                    path.to_string_lossy().into_owned()
                })
                .map_err(|error| error.to_string())
        })
        .transpose()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSaveOutcome {
    themes: Vec<themes::ThemeInfo>,
    document: String,
    path: String,
    file_error: Option<String>,
}

#[tauri::command(async)]
pub fn save_theme_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    overwrite: bool,
    path: String,
) -> Result<ThemeSaveOutcome, String> {
    // Reject a bad target before creating the local theme.
    target_path(&path)?;
    let (active, document) =
        super::themes::save_theme_file(&state.paths, &state.watcher, &name, overwrite)?;
    if active {
        super::themes::emit_theme_if_active(&app, &state.paths, &state.watcher, &name);
    }
    let file_error = write_theme_copy(&path, &document).err();
    if let Some(error) = &file_error {
        tracing::warn!(%error, "theme saved locally; writing its external copy failed");
    }
    Ok(ThemeSaveOutcome {
        themes: themes::summaries(&state.paths, &state.watcher.current()),
        document,
        path,
        file_error,
    })
}

#[tauri::command(async)]
pub fn theme_file_document(state: State<'_, AppState>, name: String) -> Result<String, String> {
    theme_io::export_document_json(&state.paths, &name).map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn write_theme_copy(path: &str, document: &str) -> Result<(), String> {
    let target = target_path(path)?;
    theme_io::write_copy(&target, document).map_err(|error| error.to_string())
}

fn target_path(path: &str) -> Result<PathBuf, String> {
    let target = PathBuf::from(path);
    if !target.is_absolute()
        || target
            .extension()
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("json"))
    {
        return Err("Choose an absolute filename ending in .json".into());
    }
    Ok(target)
}
