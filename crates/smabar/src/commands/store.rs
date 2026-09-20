//! Community Store commands: thin wrappers over the core's `StoreService`,
//! plus the two things only the app edge can do — emit events to the
//! webviews and run the background refresh.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use smabar_core::store::{
    InstallPhase, InstallProgress, ItemKind, StoreDetail, StoreEvent, StoreOverview, StoreService,
};
use tauri::{AppHandle, Emitter, State};

use super::AppState;

/// First check after startup, then every six hours — the rhythm of the app
/// updater (`shell/src/ipc/update.ts`), staggered behind it.
const FIRST_REFRESH: Duration = Duration::from_secs(90);
const REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Download progress reaches the shell at most once per this many bytes.
const PROGRESS_STEP: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    reason: smabar_core::store::ChangeReason,
    catalog_state: smabar_core::store::CatalogState,
}

/// The catalog joined with what is installed, from memory; no network.
#[tauri::command]
pub fn store_overview(state: State<'_, AppState>) -> StoreOverview {
    state.store().overview()
}

/// Fetches the catalog and returns the new overview; an unreachable store is
/// reported in `catalogState`/`lastError`, never as an error.
#[tauri::command]
pub async fn store_refresh(state: State<'_, AppState>) -> Result<StoreOverview, String> {
    Ok(state.store().refresh().await)
}

#[tauri::command]
pub async fn store_detail(
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> Result<StoreDetail, String> {
    state
        .store()
        .detail(kind, &id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn store_theme_preview(
    state: State<'_, AppState>,
    name: String,
    expected_commit: String,
) -> Result<smabar_core::themes::ThemePreview, String> {
    state
        .store()
        .theme_preview(&name, &expected_commit)
        .await
        .map_err(|error| error.to_string())
}

/// Installing from the panel needs the accepted terms of use (ADR 0017);
/// the MCP tools are not gated (ADR 0001).
pub(super) fn require_terms(state: &AppState) -> Result<(), String> {
    if smabar_core::legal::is_accepted(state.paths()) {
        return Ok(());
    }
    Err(
        "the terms of use are not accepted yet; accept them under Settings › Legal, then install again"
            .to_string(),
    )
}

/// Installs or updates a plugin after an explicit confirmation in the
/// settings panel; progress arrives as `store-progress` events.
#[tauri::command]
pub async fn store_install_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    expected_version: String,
    confirm_modified: bool,
) -> Result<StoreOverview, String> {
    require_terms(&state)?;
    let gate = Mutex::new(ProgressGate::default());
    let sink = |progress: InstallProgress| {
        let mut gate = gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if progress.phase != InstallPhase::Downloading || gate.should_emit(progress.received) {
            let _ = app.emit("store-progress", &progress);
        }
    };
    let store = state.store();
    let outcome = store
        .install_plugin(&id, &expected_version, confirm_modified, &sink)
        .await
        .map_err(|error| error.to_string())?;
    if !outcome.settled {
        tracing::info!(
            plugin = %id,
            "installed from the store; the plugin is still starting (a first python run provisions its toolchain)"
        );
    }
    Ok(store.overview())
}

/// Installs or updates a theme after an explicit confirmation.
#[tauri::command]
pub async fn store_install_theme(
    state: State<'_, AppState>,
    name: String,
    expected_version: String,
) -> Result<StoreOverview, String> {
    require_terms(&state)?;
    let store = state.store();
    store
        .install_theme(&name, &expected_version)
        .await
        .map_err(|error| error.to_string())?;
    Ok(store.overview())
}

/// Forwards store changes to every webview as `store-changed`.
pub fn spawn_store_events(app: AppHandle, store: &StoreService) {
    let mut events = store.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(StoreEvent {
                    reason,
                    catalog_state,
                }) => {
                    let _ = app.emit(
                        "store-changed",
                        ChangedPayload {
                            reason,
                            catalog_state,
                        },
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Refreshes the catalog in the background so the blocklist is enforced and
/// updates are noticed without anyone opening the store.
pub fn spawn_store_refresh_timer(store: StoreService) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_REFRESH).await;
        loop {
            store.refresh().await;
            tokio::time::sleep(REFRESH_INTERVAL).await;
        }
    });
}

/// Lets the first chunk through, then one event per [`PROGRESS_STEP`].
#[derive(Default)]
struct ProgressGate {
    last: u64,
}

impl ProgressGate {
    fn should_emit(&mut self, received: u64) -> bool {
        if self.last == 0 || received.saturating_sub(self.last) >= PROGRESS_STEP {
            self.last = received;
            return true;
        }
        false
    }
}
