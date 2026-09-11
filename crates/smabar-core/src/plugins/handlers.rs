//! Handling of plugin→core JSON-RPC traffic inside the supervision loop.

use std::collections::HashMap;

use serde_json::{Value, json};
use tokio::task::JoinHandle;

use crate::providers::{AudioAction, MediaAction, ProviderConfig, ProviderKind, Subscription};

use super::PluginEvent;
use super::logfile::PluginLog;
use super::rpc::RpcClient;
use super::runner::RunCtx;

const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const PROVIDER_OPERATION_FAILED: i64 = -32000;
const PROVIDER_INTERVAL_MIN_MS: u64 = 250;
const PROVIDER_INTERVAL_MAX_MS: u64 = 3_600_000;
const INVALID_TTL_FALLBACK_MS: u32 = 5_000;
const INVALID_TTL_WARNING: &str = "ui.render received an invalid ttlMs value or a misspelled ttlMs field and used 5000 ms; use exact ttlMs with null for a sticky popup or an unsigned 32-bit integer for auto-dismiss";
const TTL_ALIAS_WARNING: &str = "ui.render ignored a misspelled ttlMs field and kept the exact ttlMs value; field names are case-sensitive, so use ttlMs";

struct MediaActionRequest {
    action: MediaAction,
    session_id: Option<String>,
}

enum ProviderActionRequest {
    Media(MediaActionRequest),
    Audio(AudioAction),
}

/// Reports adjusted or rejected plugin input in the PLUGIN's own log.
///
/// Source `core` distinguishes host diagnostics from the plugin's own output.
/// `plugin_logs(id)` returns this log together with tagged entries from the
/// shared core log; warn_once also bounds repeated diagnostics.
fn reject(ctx: &RunCtx, message: String) {
    ctx.diagnostics.warn_once(&ctx.manifest.id, &message, None);
}

/// Handles one notification from the plugin.
pub(crate) fn handle_notification(ctx: &RunCtx, log: &mut PluginLog, method: &str, params: &Value) {
    match method {
        "ui.render" => {
            let tile_id = field_str(params, "tileId");
            let target = field_str(params, "target");
            let html = field_str(params, "html");
            let (Some(tile_id), Some(target), Some(html)) = (tile_id, target, html) else {
                reject(ctx, "ui.render was ignored: tileId, target, and html must all be strings; use app.render(tile_id, \"tile\" | \"flyout\" | \"hover\" | \"popup\", html)".to_string());
                return;
            };
            if !is_render_target(target) {
                reject(
                    ctx,
                    format!(
                        "ui.render target {target:?} was ignored; use tile, flyout, hover, or popup"
                    ),
                );
                return;
            }
            if !ctx.manifest.tiles.iter().any(|w| w.id == tile_id) {
                let declared: Vec<&str> =
                    ctx.manifest.tiles.iter().map(|w| w.id.as_str()).collect();
                reject(
                    ctx,
                    format!(
                        "ui.render dropped: tile {tile_id:?} is not declared in \
                         smabar.json (declared: {}); add it to tiles or render into a declared id",
                        declared.join(", ")
                    ),
                );
                return;
            }
            let (ttl_ms, ttl_warning) = render_ttl(params);
            if let Some(warning) = ttl_warning {
                reject(ctx, warning.to_string());
            }
            ctx.emit(PluginEvent::UiRender {
                plugin_id: ctx.manifest.id.clone(),
                tile_id: tile_id.to_string(),
                target: target.to_string(),
                html: html.to_string(),
                ttl_ms,
            });
        }
        "log" => {
            let level = field_str(params, "level")
                .filter(|level| matches!(*level, "debug" | "info" | "warn" | "error"))
                .unwrap_or("info");
            let message = field_str(params, "message").unwrap_or_default();
            log.write(level, "log", message, params.get("fields"));
        }
        other => {
            reject(
                ctx,
                format!(
                    "notification {other:?} was ignored; supported plugin notifications are ui.render and log"
                ),
            );
        }
    }
}

/// Handles one request from the plugin and always answers it.
pub(crate) async fn handle_request(
    ctx: &RunCtx,
    rpc: &RpcClient,
    forwarders: &mut HashMap<ProviderKind, JoinHandle<()>>,
    id: &Value,
    method: &str,
    params: &Value,
) {
    match method {
        "provider.subscribe" => {
            let Some(kind) = field_str(params, "kind").and_then(ProviderKind::parse) else {
                let expected = ctx.hub.available_names().join(", ");
                rpc.respond_error(
                    id,
                    INVALID_PARAMS,
                    &format!("unknown provider kind; expected one of: {expected}"),
                )
                .await;
                return;
            };
            if !ctx.hub.supports(kind) {
                let expected = ctx.hub.available_names().join(", ");
                rpc.respond_error(
                    id,
                    INVALID_PARAMS,
                    &format!(
                        "provider {:?} is not available on this platform; expected one of: {expected}",
                        kind.as_str()
                    ),
                )
                .await;
                return;
            }
            let interval_ms = match provider_interval(params) {
                Ok(interval_ms) => interval_ms,
                Err(message) => {
                    rpc.respond_error(id, INVALID_PARAMS, message).await;
                    return;
                }
            };
            if forwarders.contains_key(&kind) {
                rpc.respond_error(
                    id,
                    INVALID_PARAMS,
                    "provider kind is already subscribed; subscribe once per kind",
                )
                .await;
                return;
            }
            let subscription = ctx
                .hub
                .subscribe(ProviderConfig { kind, interval_ms })
                .await;
            forwarders.insert(kind, spawn_forwarder(subscription, rpc.clone()));
            rpc.respond_ok(id, json!({})).await;
        }
        "provider.action" => {
            let request = match parse_provider_action_request(params) {
                Ok(request) => request,
                Err(message) => {
                    reject(ctx, format!("provider.action was not run: {message}"));
                    rpc.respond_error(id, INVALID_PARAMS, message).await;
                    return;
                }
            };
            match request {
                ProviderActionRequest::Media(request) => {
                    if !ctx.hub.supports(ProviderKind::Media) {
                        rpc.respond_error(
                            id,
                            INVALID_PARAMS,
                            "the media provider is not available on this platform",
                        )
                        .await;
                        return;
                    }
                    match ctx
                        .hub
                        .media_action(request.session_id.as_deref(), request.action)
                        .await
                    {
                        Ok(()) => rpc.respond_ok(id, json!({})).await,
                        Err(error) => {
                            tracing::warn!(plugin = %ctx.manifest.id, %error, "provider action failed");
                            rpc.respond_error(
                                id,
                                PROVIDER_OPERATION_FAILED,
                                &format!("media action failed: {error}"),
                            )
                            .await;
                        }
                    }
                }
                ProviderActionRequest::Audio(action) => {
                    if !ctx.hub.supports(ProviderKind::Audio) {
                        rpc.respond_error(
                            id,
                            INVALID_PARAMS,
                            "the audio provider is not available on this platform",
                        )
                        .await;
                        return;
                    }
                    match ctx.hub.audio_action(action).await {
                        Ok(()) => rpc.respond_ok(id, json!({})).await,
                        Err(error) => {
                            tracing::warn!(plugin = %ctx.manifest.id, %error, "provider action failed");
                            rpc.respond_error(
                                id,
                                PROVIDER_OPERATION_FAILED,
                                &format!("audio action failed: {error}"),
                            )
                            .await;
                        }
                    }
                }
            }
        }
        "settings.get" => {
            rpc.respond_ok(id, json!({"settings": ctx.current_settings()}))
                .await;
        }
        "settings.set" => {
            let Some(settings) = params.get("settings").filter(|value| value.is_object()) else {
                rpc.respond_error(
                    id,
                    INVALID_PARAMS,
                    "settings.set requires \"settings\" to be a JSON object",
                )
                .await;
                return;
            };
            let plugin_id = ctx.manifest.id.clone();
            match ctx.config.update(|current| {
                let mut updated = current.clone();
                updated.plugins.insert(plugin_id, settings.clone());
                (updated, ())
            }) {
                Ok(()) => rpc.respond_ok(id, json!({})).await,
                Err(error) => {
                    rpc.respond_error(
                        id,
                        INTERNAL_ERROR,
                        &format!("saving settings failed: {error}"),
                    )
                    .await;
                }
            }
        }
        other => {
            rpc.respond_error(id, METHOD_NOT_FOUND, &format!("unknown method \"{other}\""))
                .await;
        }
    }
}

fn provider_interval(params: &Value) -> Result<u64, &'static str> {
    match params.get("intervalMs") {
        None => Ok(0),
        Some(value) => value
            .as_u64()
            .filter(|interval| {
                *interval == 0
                    || (PROVIDER_INTERVAL_MIN_MS..=PROVIDER_INTERVAL_MAX_MS).contains(interval)
            })
            .ok_or(
                "provider.subscribe intervalMs must be 0 (the provider default) or an integer from 250 to 3600000",
            ),
    }
}

fn parse_provider_action_request(params: &Value) -> Result<ProviderActionRequest, &'static str> {
    match params.get("kind").and_then(Value::as_str) {
        Some("media") => parse_media_action_request(params).map(ProviderActionRequest::Media),
        Some("audio") => parse_audio_action_request(params).map(ProviderActionRequest::Audio),
        _ => Err("provider.action kind must be \"media\" or \"audio\""),
    }
}

fn parse_media_action_request(params: &Value) -> Result<MediaActionRequest, &'static str> {
    let Some(params) = params.as_object() else {
        return Err("provider.action params must be an object");
    };
    if params
        .keys()
        .any(|key| !matches!(key.as_str(), "kind" | "action" | "sessionId"))
    {
        return Err("provider.action accepts only kind, action, and optional sessionId");
    }
    if params.get("kind").and_then(Value::as_str) != Some("media") {
        return Err("provider.action kind must be \"media\"");
    }
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .and_then(MediaAction::parse)
        .ok_or("provider.action action must be play, pause, playPause, next, or previous")?;
    let session_id = match params.get("sessionId") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.is_empty() && value.chars().count() <= 512 => {
            Some(value.clone())
        }
        Some(Value::String(_)) => {
            return Err(
                "provider.action sessionId must be a non-empty string up to 512 characters",
            );
        }
        Some(_) => return Err("provider.action sessionId must be a string or null"),
    };
    Ok(MediaActionRequest { action, session_id })
}

fn parse_audio_action_request(params: &Value) -> Result<AudioAction, &'static str> {
    let Some(params) = params.as_object() else {
        return Err("provider.action params must be an object");
    };
    if params.get("kind").and_then(Value::as_str) != Some("audio") {
        return Err("provider.action kind must be \"audio\"");
    }
    match params.get("action").and_then(Value::as_str) {
        Some("setVolume") => {
            if params
                .keys()
                .any(|key| !matches!(key.as_str(), "kind" | "action" | "volumePercent"))
            {
                return Err("audio setVolume accepts only kind, action, and volumePercent");
            }
            let percent = params
                .get("volumePercent")
                .and_then(Value::as_u64)
                .filter(|value| *value <= 100)
                .and_then(|value| u8::try_from(value).ok())
                .ok_or("audio setVolume volumePercent must be an integer from 0 to 100")?;
            Ok(AudioAction::SetVolume(percent))
        }
        Some("setMuted") => {
            if params
                .keys()
                .any(|key| !matches!(key.as_str(), "kind" | "action" | "muted"))
            {
                return Err("audio setMuted accepts only kind, action, and muted");
            }
            let muted = params
                .get("muted")
                .and_then(Value::as_bool)
                .ok_or("audio setMuted muted must be a boolean")?;
            Ok(AudioAction::SetMuted(muted))
        }
        _ => Err("audio action must be setVolume or setMuted"),
    }
}

fn field_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn optional_u32(params: &Value, key: &str) -> Result<Option<u32>, ()> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or(()),
    }
}

fn render_ttl(params: &Value) -> (Option<u32>, Option<&'static str>) {
    let exact_present = params.get("ttlMs").is_some();
    let mismatched = params
        .as_object()
        .is_some_and(|params| params.keys().any(|key| key != "ttlMs" && is_ttl_alias(key)));
    match optional_u32(params, "ttlMs") {
        Err(()) => (Some(INVALID_TTL_FALLBACK_MS), Some(INVALID_TTL_WARNING)),
        Ok(value) if exact_present && mismatched => (value, Some(TTL_ALIAS_WARNING)),
        Ok(None) if mismatched => (Some(INVALID_TTL_FALLBACK_MS), Some(INVALID_TTL_WARNING)),
        Ok(value) => (value, None),
    }
}

fn is_ttl_alias(key: &str) -> bool {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .eq("ttlms".chars())
}

fn is_render_target(target: &str) -> bool {
    matches!(target, "tile" | "flyout" | "hover" | "popup")
}

/// Forwards provider events to the plugin as `provider.data` notifications
/// until the task is aborted (plugin stop drops the subscription and thereby
/// the sampler refcount) or the hub closes.
fn spawn_forwarder(mut subscription: Subscription, rpc: RpcClient) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = subscription.recv().await {
            if !rpc
                .notify(
                    "provider.data",
                    json!({"kind": event.kind.as_str(), "data": event.data, "tsMs": event.ts_ms}),
                )
                .await
            {
                return;
            }
        }
    })
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
