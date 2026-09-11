//! The legal notice: status for the settings window, the acceptance that
//! releases the bar, and the decline that quits smabar (the tray's quit path).

use serde::Serialize;
use smabar_core::legal::{self, LegalStatus};
use tauri::{AppHandle, Emitter, State};

use super::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegalChanged {
    required: bool,
}

/// The bundled texts in the configured language plus the acceptance state.
#[tauri::command]
pub fn legal_status(state: State<'_, AppState>) -> LegalStatus {
    legal::status(state.paths(), &state.config().language)
}

/// Records the acceptance of the bundled terms version and tells every
/// window (`legal-changed`) that the bar may show its tiles again.
#[tauri::command]
pub fn legal_accept(app: AppHandle, state: State<'_, AppState>) -> Result<LegalStatus, String> {
    let status = legal::accept(state.paths(), &state.config().language).map_err(|error| {
        format!(
            "cannot save the acceptance to {}: {error}; make the folder writable and accept again",
            state.paths().legal_file().display()
        )
    })?;
    tracing::info!(version = %status.terms_version, "terms of use accepted");
    let changed = LegalChanged {
        required: status.required,
    };
    if let Err(error) = app.emit("legal-changed", changed) {
        tracing::warn!(%error, "could not announce the accepted terms; the bar picks them up at the next start");
    }
    Ok(status)
}

/// Quits smabar without recording anything; the notice returns at the next
/// start. Closing the settings window only hides it, so this is the way out.
#[tauri::command]
pub fn legal_decline(app: AppHandle) {
    tracing::info!("terms of use declined; quitting smabar");
    crate::tray::release_platform_state();
    app.exit(0);
}
