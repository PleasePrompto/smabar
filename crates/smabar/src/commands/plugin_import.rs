//! Settings ZIP import: native paths and progress stay at the app edge.
use super::AppState;
use smabar_core::store::{InstallOutcome, LocalPluginPreview};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn inspect_plugin_zip(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<LocalPluginPreview, String> {
    super::store::require_terms(&state)?;
    if !path.is_absolute() {
        return Err("Choose an absolute ZIP path".into());
    }
    state
        .store()
        .inspect_local_plugin(path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn install_plugin_zip(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    archive_sha256: String,
    previous_digest: Option<String>,
) -> Result<InstallOutcome, String> {
    super::store::require_terms(&state)?;
    if !path.is_absolute() {
        return Err("Choose an absolute ZIP path".into());
    }
    state
        .store()
        .install_local_plugin(
            path,
            &archive_sha256,
            previous_digest.as_deref(),
            &|progress| {
                if let Err(error) = app.emit_to("settings", "store-progress", progress) {
                    tracing::warn!(%error, "could not report ZIP install progress");
                }
            },
        )
        .await
        .map_err(|error| error.to_string())
}
