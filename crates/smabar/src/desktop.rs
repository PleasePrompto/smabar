//! Desktop services requested by plugins; callbacks always retain session identity.
use anyhow::Context;
use serde::Serialize;
use serde_json::{Value, json};
use smabar_core::config::ConfigWatcher;
use smabar_core::plugins::{HostRequest, HostSession};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::desktop_types::{PopupId, PopupShow, valid_id};
use crate::platform::audio::AudioService;

const MAX_POPUPS: usize = 50;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupView {
    pub instance_id: u64,
    pub generation: u64,
    pub plugin_id: String,
    #[serde(flatten)]
    pub content: PopupShow,
}

struct PopupRecord {
    view: PopupView,
    owner: HostSession,
    shown: bool,
}

#[derive(Default)]
struct PopupState {
    next: u64,
    records: Vec<PopupRecord>,
}

pub struct DesktopServices {
    popups: Mutex<PopupState>,
    audio: AudioService,
    config: Arc<ConfigWatcher>,
}

impl DesktopServices {
    pub async fn action(
        &self,
        instance_id: u64,
        plugin: &str,
        tile: &str,
        action: &str,
        value: Option<Value>,
    ) -> Result<(), String> {
        let owner = self
            .popups
            .lock()
            .map_err(|_| "popup state unavailable")?
            .records
            .iter()
            .find(|r| {
                r.view.instance_id == instance_id
                    && r.view.plugin_id == plugin
                    && r.view.content.tile_id == tile
            })
            .map(|r| r.owner.clone())
            .ok_or("popup is no longer active")?;
        let mut params = json!({"tileId":tile,"action":action});
        if let Some(value) = value {
            params["value"] = value;
        }
        if !owner.notify("event", params).await {
            return Err("popup's plugin session ended; open its current tile".into());
        }
        Ok(())
    }

    pub fn start(
        app: &AppHandle,
        mut requests: tokio::sync::mpsc::Receiver<HostRequest>,
        config: Arc<ConfigWatcher>,
    ) -> anyhow::Result<()> {
        let audio = AudioService::start(config.clone())?;
        app.manage(Self {
            popups: Mutex::new(PopupState::default()),
            audio,
            config,
        });
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut cleanup = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                tokio::select! {
                    request = requests.recv() => {
                        let Some(request) = request else { break; };
                        let service = handle.state::<DesktopServices>();
                        if request.reply.is_closed() || request.session.stopped.is_cancelled() { continue; }
                        let result = service.request(&handle, &request).await.map_err(|error| error.to_string());
                        let _ = request.reply.send(result);
                    }
                    _ = cleanup.tick() => {
                        if let Err(error) = handle.state::<DesktopServices>().cleanup(&handle).await {
                            tracing::error!(%error, "could not clean up plugin notifications");
                        }
                    }
                }
            }
        });
        Ok(())
    }

    async fn request(&self, app: &AppHandle, request: &HostRequest) -> anyhow::Result<Value> {
        if request.method.starts_with("audio.") {
            return self
                .audio
                .request(
                    request.session.clone(),
                    &request.method,
                    request.params.clone(),
                    false,
                )
                .await
                .map_err(anyhow::Error::msg);
        }
        match request.method.as_str() {
            "ui.popup.show" => self.show(app, request).await,
            "ui.popup.dismiss" => {
                let args: PopupId = serde_json::from_value(request.params.clone())?;
                self.validate_tile(&request.session, &args.tile_id, &args.popup_id)?;
                let record = {
                    let mut state = self
                        .popups
                        .lock()
                        .map_err(|_| anyhow::anyhow!("popup state unavailable"))?;
                    state
                        .records
                        .iter()
                        .position(|r| {
                            r.view.generation == request.session.generation
                                && r.view.content.tile_id == args.tile_id
                                && r.view.content.popup_id == args.popup_id
                        })
                        .map(|index| state.records.remove(index))
                };
                if let Some(record) = record {
                    popup_event(&record, "dismissed", Some("plugin")).await;
                    self.changed(app)?;
                }
                Ok(json!({"state": "dismissed"}))
            }
            _ => anyhow::bail!("unknown popup method; use ui.popup.show or ui.popup.dismiss"),
        }
    }

    fn validate_tile(&self, owner: &HostSession, tile: &str, popup: &str) -> anyhow::Result<()> {
        if !owner.tiles.iter().any(|w| w == tile) || !valid_id(popup) {
            anyhow::bail!(
                "popup needs a declared tileId and a 1–128 character popupId using letters, digits, '.', '_' or '-'"
            );
        }
        Ok(())
    }

    async fn show(&self, app: &AppHandle, request: &HostRequest) -> anyhow::Result<Value> {
        let content: PopupShow = serde_json::from_value(request.params.clone())?;
        self.validate_tile(&request.session, &content.tile_id, &content.popup_id)?;
        if content.html.len() > 512 * 1024 {
            anyhow::bail!("popup HTML exceeds 512 KiB; shorten the content");
        }
        if !self.config.current().popups.enabled {
            request.session.notify("ui.popup.event", json!({"tileId":content.tile_id,"popupId":content.popup_id,"state":"suppressed"})).await;
            return Ok(json!({"state":"suppressed"}));
        }
        // Build the native surface before accepting: failure must reach the caller.
        app.state::<crate::surfaces::SurfaceManager>()
            .prepare_notifications(app)?;
        let mut dropped = None;
        let view = {
            let mut state = self
                .popups
                .lock()
                .map_err(|_| anyhow::anyhow!("popup state unavailable"))?;
            if let Some(record) = state.records.iter_mut().find(|r| {
                r.view.generation == request.session.generation
                    && r.view.content.tile_id == content.tile_id
                    && r.view.content.popup_id == content.popup_id
            }) {
                record.view.content = content;
                record.view.clone()
            } else {
                if state.records.len() >= MAX_POPUPS {
                    let index = state.records.iter().position(|r| !r.shown).unwrap_or(0);
                    dropped = Some(state.records.remove(index));
                }
                state.next += 1;
                let view = PopupView {
                    instance_id: state.next,
                    generation: request.session.generation,
                    plugin_id: request.session.plugin_id.clone(),
                    content,
                };
                state.records.push(PopupRecord {
                    view: view.clone(),
                    owner: request.session.clone(),
                    shown: false,
                });
                view
            }
        };
        if let Some(record) = dropped {
            popup_event(&record, "dropped", Some("queueFull")).await;
        }
        self.changed(app)?;
        Ok(json!({"state":"accepted", "instanceId": view.instance_id}))
    }

    fn changed(&self, app: &AppHandle) -> anyhow::Result<()> {
        // All windows may observe settings, but only notifications consumes this snapshot.
        app.emit("managed-popups-changed", ())
            .context("could not announce updated popups")
    }

    async fn cleanup(&self, app: &AppHandle) -> anyhow::Result<()> {
        let enabled = self.config.current().popups.enabled;
        let removed = {
            let mut state = self
                .popups
                .lock()
                .map_err(|_| anyhow::anyhow!("popup state unavailable"))?;
            let mut removed = Vec::new();
            let mut index = 0;
            while index < state.records.len() {
                if !enabled || state.records[index].owner.stopped.is_cancelled() {
                    removed.push(state.records.remove(index));
                } else {
                    index += 1;
                }
            }
            removed
        };
        if !removed.is_empty() {
            for record in removed {
                popup_event(&record, "suppressed", Some("disabled")).await;
            }
            self.changed(app)?;
        }
        Ok(())
    }
}

async fn popup_event(record: &PopupRecord, state: &str, reason: Option<&str>) {
    record.owner.notify("ui.popup.event", json!({"tileId":record.view.content.tile_id,
        "popupId":record.view.content.popup_id,"instanceId":record.view.instance_id,"state":state,"reason":reason})).await;
}

#[tauri::command]
pub fn get_managed_popups(service: State<'_, DesktopServices>) -> Result<Vec<PopupView>, String> {
    let state = service
        .popups
        .lock()
        .map_err(|_| "popup state unavailable")?;
    Ok(state
        .records
        .iter()
        .filter(|r| !r.owner.stopped.is_cancelled())
        .map(|r| r.view.clone())
        .collect())
}

#[tauri::command]
pub fn get_audio_settings(service: State<'_, DesktopServices>) -> smabar_core::config::AudioConfig {
    service.config.current().audio.clone()
}

#[tauri::command]
pub async fn popup_event_report(
    app: AppHandle,
    window: tauri::WebviewWindow,
    service: State<'_, DesktopServices>,
    instance_id: u64,
    state: String,
) -> Result<(), String> {
    if window.label() != crate::surfaces::SurfaceRole::Notifications.label() {
        return Err("popup events must originate from the notifications surface".into());
    }
    if !matches!(
        state.as_str(),
        "shown" | "dismissed" | "expired" | "dropped"
    ) {
        return Err("unsupported popup state".into());
    }
    let record = {
        let mut popups = service
            .popups
            .lock()
            .map_err(|_| "popup state unavailable")?;
        let Some(index) = popups
            .records
            .iter()
            .position(|r| r.view.instance_id == instance_id)
        else {
            return Ok(());
        };
        if state == "shown" {
            let record = &mut popups.records[index];
            if record.shown || record.owner.stopped.is_cancelled() {
                return Ok(());
            }
            record.shown = true;
            PopupRecord {
                view: record.view.clone(),
                owner: record.owner.clone(),
                shown: true,
            }
        } else {
            popups.records.remove(index)
        }
    };
    popup_event(&record, &state, Some("surface")).await;
    if state == "shown" {
        if let Some(source) = &record.view.content.sound {
            let params = json!({"playbackId":format!("popup-{}",record.view.instance_id),"source":source,"volume":100,"loop":false});
            if let Err(error) = service
                .audio
                .request(record.owner.clone(), "audio.play", params, true)
                .await
            {
                tracing::warn!(plugin = %record.owner.plugin_id, %error, "popup sound failed; notification remains visible");
                record.owner.notify("audio.event", json!({"state":"error","popupId":record.view.content.popup_id,"error":error})).await;
            }
        }
    } else {
        service.changed(&app).map_err(|e| e.to_string())?;
    }
    Ok(())
}
