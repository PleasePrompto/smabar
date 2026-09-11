//! Serves `BarRequest`s from the MCP layer: asks the shell to stage the
//! subject, snapshots the webview, and hands the PNG back.
//!
//! The shell is the only place that knows the layout, so every request is a
//! round trip: an event out, a `bar_reply` command back. Requests are keyed
//! by id because a slow reply must not be matched to the next request.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use smabar_core::capture::{BarError, BarPort, BarRequest, BarResponse, Screenshot, UiAction};
use smabar_core::platform::Rect;
use tauri::{AppHandle, Emitter, EventTarget, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::platform;

/// How long the shell gets to measure and answer. Shorter than the MCP-side
/// timeout so a stuck shell still releases the capture stage in time.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const TARGET_RETRY_DELAY: Duration = Duration::from_millis(50);
const TARGET_RETRY_COUNT: usize = 30;

/// What the shell reports back about one request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BarReply {
    id: u64,
    /// The subject's box in CSS pixels; absent when `error` is set.
    rect: Option<Rect>,
    /// Clipping had to be lifted to fit scrollable content in.
    #[serde(default)]
    expanded: bool,
    /// Something still clips: the picture is incomplete.
    #[serde(default)]
    clipped: bool,
    /// Set when the shell could not stage the request.
    error: Option<String>,
    /// Everything the shell could address right now.
    #[serde(default)]
    targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureRequestEvent {
    id: u64,
    target: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCommandEvent {
    id: u64,
    action: UiAction,
    tile_id: Option<String>,
    group: Option<String>,
}

/// In-flight round trips, keyed by request id.
#[derive(Default)]
pub struct CaptureState {
    pending: Mutex<HashMap<u64, oneshot::Sender<BarReply>>>,
    next_id: AtomicU64,
}

impl CaptureState {
    fn begin<T>(
        &self,
        window: Option<T>,
    ) -> Result<(T, u64, oneshot::Receiver<BarReply>), BarError> {
        // Resolve the surface before registering a round trip in either path.
        let window = window.ok_or(BarError::Disconnected)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.lock().insert(id, tx);
        Ok((window, id, rx))
    }

    fn resolve(&self, reply: BarReply) {
        if let Some(sender) = self.lock().remove(&reply.id) {
            let _ = sender.send(reply);
        }
    }

    fn forget(&self, id: u64) {
        self.lock().remove(&id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<BarReply>>> {
        // A panicking holder only ever left a HashMap behind — the map is
        // still consistent, so the poison flag carries no information here.
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The shell's answer to a `bar-capture` / `bar-ui-command` event.
#[tauri::command]
pub fn bar_reply(state: State<'_, Arc<CaptureState>>, reply: BarReply) {
    state.resolve(reply);
}

/// Drains the MCP port for the process lifetime.
pub fn spawn(app: AppHandle, mut rx: mpsc::UnboundedReceiver<BarEnvelope>) {
    tauri::async_runtime::spawn(async move {
        while let Some((request, responder)) = rx.recv().await {
            let result = match request {
                BarRequest::UiState {
                    action,
                    tile_id,
                    group,
                } => ui_state(&app, action, tile_id, group).await,
                BarRequest::Screenshot { target, scale } => screenshot(&app, &target, scale).await,
            };
            let _ = responder.send(result);
        }
        tracing::info!("bar capture port closed");
    });
}

/// The envelope shape `BarPort::channel` produces.
pub type BarEnvelope = (BarRequest, oneshot::Sender<Result<BarResponse, BarError>>);

async fn screenshot(app: &AppHandle, target: &str, scale: u8) -> Result<BarResponse, BarError> {
    for attempt in 0..TARGET_RETRY_COUNT {
        let result = capture_once(app, target, scale).await;
        if !matches!(result, Err(BarError::UnknownTarget { .. }))
            || attempt + 1 == TARGET_RETRY_COUNT
        {
            return result;
        }
        tokio::time::sleep(TARGET_RETRY_DELAY).await;
    }
    Err(BarError::Timeout(
        TARGET_RETRY_DELAY * u32::try_from(TARGET_RETRY_COUNT).unwrap_or(u32::MAX),
    ))
}

async fn capture_once(app: &AppHandle, target: &str, scale: u8) -> Result<BarResponse, BarError> {
    let state = app.state::<Arc<CaptureState>>();
    let (window, id, rx) = state.begin(capture_window(app, target))?;
    if let Err(error) = window.emit_to(
        EventTarget::webview_window(window.label()),
        "bar-capture",
        CaptureRequestEvent {
            id,
            target: target.to_string(),
        },
    ) {
        // begin()/forget() must stay symmetrical on every path, even this
        // shutdown-adjacent one — otherwise a dead oneshot sender leaks.
        state.forget(id);
        return Err(BarError::Failed(error.to_string()));
    }

    let reply = match await_reply(&state, id, rx).await {
        Ok(reply) => reply,
        Err(error) => {
            // The shell may still be preparing a retried target. Cancel that
            // transaction too, so it cannot stage layout after our timeout.
            let _ = window.emit_to(
                EventTarget::webview_window(window.label()),
                "bar-capture-release",
                id,
            );
            return Err(error);
        }
    };
    // Whatever happens from here, the shell must get its layout back.
    let outcome = render(&window, target, scale, &reply).await;
    let _ = window.emit_to(
        EventTarget::webview_window(window.label()),
        "bar-capture-release",
        id,
    );
    outcome
}

fn capture_window(app: &AppHandle, target: &str) -> Option<tauri::WebviewWindow> {
    app.get_webview_window(capture_label(target))
}

fn capture_label(target: &str) -> &'static str {
    match target {
        "flyout" => "overlay",
        "settings" => "settings",
        "popup" => "notifications",
        _ => "bar",
    }
}

async fn render(
    window: &tauri::WebviewWindow,
    target: &str,
    scale: u8,
    reply: &BarReply,
) -> Result<BarResponse, BarError> {
    if let Some(error) = &reply.error {
        return Err(shell_error(error, target, &reply.targets));
    }
    let rect = reply
        .rect
        .ok_or_else(|| BarError::Failed("the shell measured no rectangle".into()))?;
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let (png, width, height) =
        platform::capture::snapshot_png(window, rect, scale_factor, scale).await?;
    Ok(BarResponse::Screenshot(Box::new(Screenshot {
        png,
        rect,
        pixels: (width, height),
        expanded: reply.expanded,
        clipped: reply.clipped,
        targets: reply.targets.clone(),
    })))
}

async fn ui_state(
    app: &AppHandle,
    action: UiAction,
    tile_id: Option<String>,
    group: Option<String>,
) -> Result<BarResponse, BarError> {
    let state = app.state::<Arc<CaptureState>>();
    let (window, id, rx) = state.begin(app.get_webview_window("bar"))?;
    let subject = tile_id.clone().unwrap_or_default();
    if let Err(error) = window.emit_to(
        EventTarget::webview_window(window.label()),
        "bar-ui-command",
        UiCommandEvent {
            id,
            action,
            tile_id,
            group,
        },
    ) {
        // Same symmetry as in `screenshot`: never leak the pending entry.
        state.forget(id);
        return Err(BarError::Failed(error.to_string()));
    }
    let reply = await_reply(&state, id, rx).await?;
    if let Some(error) = &reply.error {
        return Err(shell_error(error, &subject, &reply.targets));
    }
    Ok(BarResponse::UiState {
        targets: reply.targets.clone(),
    })
}

/// `unknown-target` is the one shell failure the caller can act on, so it
/// keeps its own variant carrying the list of targets that DO exist.
fn shell_error(error: &str, target: &str, targets: &[String]) -> BarError {
    if error == "unknown-target" {
        BarError::UnknownTarget {
            target: target.to_string(),
            available: targets.to_vec(),
        }
    } else {
        BarError::Failed(error.to_string())
    }
}

async fn await_reply(
    state: &Arc<CaptureState>,
    id: u64,
    rx: oneshot::Receiver<BarReply>,
) -> Result<BarReply, BarError> {
    match tokio::time::timeout(REPLY_TIMEOUT, rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => {
            state.forget(id);
            Err(BarError::Disconnected)
        }
        Err(_) => {
            state.forget(id);
            Err(BarError::Timeout(REPLY_TIMEOUT))
        }
    }
}

/// Wires the MCP port to this app instance.
pub fn install(app: &AppHandle) -> BarPort {
    let (port, rx) = BarPort::channel();
    app.manage(Arc::new(CaptureState::default()));
    spawn(app.clone(), rx);
    port
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_surfaces_do_not_register_requests_or_break_the_next_reply() {
        let state = Arc::new(CaptureState::default());
        for _ in 0..10 {
            assert!(matches!(
                state.begin(None::<()>),
                Err(BarError::Disconnected)
            ));
            assert!(state.lock().is_empty());
        }
        let (_, id, rx) = state.begin(Some(())).expect("available surface");
        state.resolve(BarReply {
            id,
            rect: None,
            expanded: false,
            clipped: false,
            error: None,
            targets: vec!["bar".into()],
        });
        assert_eq!(await_reply(&state, id, rx).await.expect("reply").id, id);
        assert!(state.lock().is_empty());

        let (_, id, rx) = state.begin(Some(())).expect("available surface");
        assert!(matches!(
            await_reply(&state, id, rx).await,
            Err(BarError::Timeout(_))
        ));
        assert!(state.lock().is_empty());
    }

    #[test]
    fn target_roles_are_stable() {
        assert_eq!(capture_label("flyout"), "overlay");
        assert_eq!(capture_label("overlay"), "bar");
        assert_eq!(capture_label("settings"), "settings");
        assert_eq!(capture_label("popup"), "notifications");
        assert_eq!(capture_label("plugin:clock:clock"), "bar");
    }
}
