//! Forwards supervisor events to the shell. Persistent renders that arrive
//! within one window travel as one event per surface: Tauri evaluates every
//! emit as its own JavaScript program with the payload as source text, so a
//! few larger events cost the webview less than one program per render.

use std::time::Duration;

use serde_json::json;
use smabar_core::plugins::{PluginEvent, PluginSupervisor};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use super::AppState;
use crate::memory_probe::Counters;
use crate::surfaces::SurfaceManager;

/// A plugin's tile, hover and flyout renders arrive within milliseconds of
/// each other; 100 ms stays below what a viewer notices on a tile.
const BATCH_WINDOW: Duration = Duration::from_millis(100);

/// Forwards supervisor events, routing persistent HTML by surface role.
pub fn spawn_plugin_events(
    app: AppHandle,
    supervisor: &PluginSupervisor,
    mut rx: Receiver<PluginEvent>,
) {
    let supervisor = supervisor.clone();
    let mut probe = Counters::new(*app.state::<crate::memory_probe::Mode>());
    tauri::async_runtime::spawn(async move {
        loop {
            let received = match rx.recv().await {
                Ok(event) if is_persistent_render(&event) => {
                    let (run, tail) = collect_run(&mut rx, event).await;
                    deliver_renders(&app, &mut probe, &run);
                    match tail {
                        Some(tail) => tail,
                        None => continue,
                    }
                }
                other => other,
            };
            match received {
                Ok(event) => forward_plugin_event(&app, &mut probe, event).await,
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "plugin event listener lagged");
                    let (snapshot, pending) = supervisor.resync_ui(&mut rx);
                    for event in pending {
                        if !is_persistent_render(&event) {
                            forward_plugin_event(&app, &mut probe, event).await;
                        }
                    }
                    if let Err(error) = app
                        .state::<SurfaceManager>()
                        .replay_plugin_ui(&app, snapshot)
                    {
                        tracing::error!(%error, "failed to restore plugin UI after event lag; reload the affected window if it stays stale");
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

fn is_persistent_render(event: &PluginEvent) -> bool {
    matches!(event, PluginEvent::UiRender { target, .. } if target != "popup")
}

/// Collects the persistent renders that follow `first` within [`BATCH_WINDOW`].
/// Any other event, lag or closure ends the run early and is handed back.
async fn collect_run(
    rx: &mut Receiver<PluginEvent>,
    first: PluginEvent,
) -> (Vec<PluginEvent>, Option<Result<PluginEvent, RecvError>>) {
    let mut run = vec![first];
    let deadline = tokio::time::Instant::now() + BATCH_WINDOW;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(event)) if is_persistent_render(&event) => run.push(event),
            Ok(tail) => return (run, Some(tail)),
            Err(_elapsed) => return (run, None),
        }
    }
}

/// Every render feeds the probe; the run is suppressed when any render is.
fn deliver_renders(app: &AppHandle, probe: &mut Counters, run: &[PluginEvent]) {
    let mut suppress = false;
    for event in run {
        suppress |= matches!(event, PluginEvent::UiRender { plugin_id, tile_id, target, html, .. }
            if probe.suppress(plugin_id, tile_id, target, html.len()));
    }
    if let Err(error) = app
        .state::<SurfaceManager>()
        .deliver_plugin_ui(app, run, suppress)
    {
        tracing::error!(%error, "failed to deliver current plugin UI; reload the affected window if it stays stale");
    }
}

async fn forward_plugin_event(app: &AppHandle, probe: &mut Counters, event: PluginEvent) {
    deliver_renders(app, probe, std::slice::from_ref(&event));
    let (channel, payload) = match event {
        PluginEvent::Added {
            plugin_id,
            name,
            icon_data_url,
            tiles,
            settings_schema,
        } => (
            "plugin-added",
            json!({
                "pluginId": plugin_id,
                "name": name,
                "iconDataUrl": icon_data_url,
                "tiles": tiles,
                "settingsSchema": settings_schema,
            }),
        ),
        PluginEvent::Removed { plugin_id } => ("plugin-removed", json!({ "pluginId": plugin_id })),
        PluginEvent::Status {
            plugin_id,
            status,
            error,
        } => (
            "plugin-status",
            json!({ "pluginId": plugin_id, "status": status, "error": error }),
        ),
        PluginEvent::UiRender { ref target, .. } if target != "popup" => return,
        PluginEvent::UiRender {
            plugin_id,
            tile_id,
            target,
            html,
            ttl_ms,
        } => {
            if !app.state::<AppState>().config().popups.enabled {
                return;
            }
            let mut payload = json!({
                "pluginId": plugin_id,
                "tileId": tile_id,
                "target": target,
                "html": html,
            });
            if let Some(ttl_ms) = ttl_ms {
                payload["ttlMs"] = json!(ttl_ms);
            }
            if let Err(error) = app
                .state::<SurfaceManager>()
                .enqueue_popup(app, payload)
                .await
            {
                tracing::error!(
                    %error,
                    %plugin_id,
                    %tile_id,
                    "failed to present plugin popup"
                );
            }
            return;
        }
    };
    if let Err(error) = app.emit(channel, payload) {
        app.state::<AppState>().plugin_delivery.invalidate();
        tracing::error!(
            %error,
            channel,
            "failed to deliver plugin event to the shell; reload the affected window if its state stays stale"
        );
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;

    fn render(html: &str) -> PluginEvent {
        PluginEvent::UiRender {
            plugin_id: "demo".to_string(),
            tile_id: "one".to_string(),
            target: "tile".to_string(),
            html: html.to_string(),
            ttl_ms: None,
        }
    }

    #[test]
    fn a_burst_is_collected_until_the_window_elapses() {
        tauri::async_runtime::block_on(async {
            let (tx, mut rx) = broadcast::channel(8);
            for html in ["A", "B", "C"] {
                tx.send(render(html)).unwrap();
            }
            let first = rx.recv().await.unwrap();
            let (run, tail) = collect_run(&mut rx, first).await;
            assert_eq!(run.len(), 3);
            assert!(tail.is_none());
        });
    }

    #[test]
    fn other_events_lag_and_closure_end_a_run_early() {
        tauri::async_runtime::block_on(async {
            let (tx, mut rx) = broadcast::channel(2);
            tx.send(render("A")).unwrap();
            tx.send(PluginEvent::Removed {
                plugin_id: "demo".to_string(),
            })
            .unwrap();
            let first = rx.recv().await.unwrap();
            let (run, tail) = collect_run(&mut rx, first).await;
            assert_eq!(run.len(), 1);
            assert!(matches!(tail, Some(Ok(PluginEvent::Removed { .. }))));

            tx.send(render("B")).unwrap();
            let first = rx.recv().await.unwrap();
            for html in ["C", "D", "E"] {
                tx.send(render(html)).unwrap();
            }
            let (run, tail) = collect_run(&mut rx, first).await;
            assert_eq!(run.len(), 1, "a lag flushes what was collected");
            assert!(matches!(tail, Some(Err(RecvError::Lagged(1)))));

            drop(tx);
            let (run, tail) = collect_run(&mut rx, render("F")).await;
            assert_eq!(run.len(), 3, "buffered renders precede the closure");
            assert!(matches!(tail, Some(Err(RecvError::Closed))));
        });
    }
}
