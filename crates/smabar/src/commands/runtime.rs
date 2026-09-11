//! Managed-Python-runtime commands and events (smabar-core's provisioner):
//! a status snapshot for late-attaching shells, a retry trigger, and the
//! `runtime-status` event bridge.

use smabar_core::plugins::{PluginSupervisor, RuntimeStatus};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::broadcast::error::RecvError;

use super::AppState;

/// Snapshot for the shell's bridge bootstrap — events only cover what
/// happens after the listener attaches.
#[tauri::command]
pub fn get_runtime_status(state: State<AppState>) -> RuntimeStatus {
    state.supervisor.runtime().status()
}

/// Kicks provisioning off again after a failure and returns immediately;
/// progress and the outcome arrive as `runtime-status` events. A successful
/// install also revives python plugins that parked while the runtime was
/// missing (the supervisor's nudge task).
#[tauri::command]
pub fn retry_provisioning(state: State<AppState>) {
    let runtime = state.supervisor.runtime();
    tauri::async_runtime::spawn(async move {
        runtime.ensure().await;
    });
}

/// Forwards provisioner transitions to the shell as `runtime-status` events.
pub fn spawn_runtime_events(app: AppHandle, supervisor: &PluginSupervisor) {
    let mut rx = supervisor.runtime().subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(status) => {
                    let _ = app.emit("runtime-status", status);
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "runtime status listener lagged");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}
