//! Turning a plugin write or restart into a REAL answer.
//!
//! Writing a plugin file only nudges the folder watcher, so the tools used to
//! reply "it will reload, check the logs yourself". An agent then had to poll,
//! and a broken manifest still looked like a success. Everything here exists to
//! give the caller the actual outcome — validated before writing, written
//! atomically, and awaited afterwards.

use std::fs;
use std::path::Path;
use std::time::Duration;

use rmcp::ErrorData as McpError;
use serde_json::{Value, json};

use crate::plugins::{PluginManifest, PluginStatus, PluginTileDef, StatusWatcher};

use super::SmabarMcp;
use super::plugin_types::{PluginLogEntry, ReloadOutcomeOut, ReloadWarnings};

/// How long a write/reload waits for the plugin to settle. A plugin's FIRST
/// python run provisions a toolchain through uv and can take far longer; that
/// case returns `settled: false` instead of a failure.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// After `running`, how long the reply waits for every declared tile to
/// push its tile. `on_ready` renders within milliseconds; a plugin that
/// fetches the network first shows up under `rendered` on the next write.
const FIRST_RENDER_GRACE: Duration = Duration::from_millis(2000);
const RENDER_POLL: Duration = Duration::from_millis(50);
/// After the tiles rendered, how long the shell's markup report needs to
/// travel back into the plugin log so the reply can carry it.
const SHELL_REPORT_GRACE: Duration = Duration::from_millis(300);
/// Log lines returned with a reload outcome.
const RELOAD_LOG_LIMIT: usize = 20;
/// Warnings summarised in a reload outcome.
const RELOAD_WARNING_LIMIT: usize = 10;
const RELOAD_WARNING_CHARS: usize = 300;

impl SmabarMcp {
    /// Writes `content` to `target` atomically (temp file + rename).
    ///
    /// The temp name starts with a dot on purpose: the folder watcher and
    /// `plugin_read` both skip dot-entries, so the intermediate file neither
    /// triggers a spurious reload nor shows up as a plugin file. A plain
    /// `fs::write` let the watcher pick up half-written scripts.
    pub(super) fn atomic_write(&self, target: &Path, content: &str) -> Result<(), McpError> {
        let parent = target.parent().ok_or_else(|| {
            McpError::internal_error(
                format!("{} has no parent directory", target.display()),
                None,
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            McpError::internal_error(format!("cannot create {}: {error}", parent.display()), None)
        })?;
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let temp = parent.join(format!(".{name}.tmp"));
        fs::write(&temp, content.as_bytes()).map_err(|error| {
            McpError::internal_error(format!("cannot write {}: {error}", temp.display()), None)
        })?;
        fs::rename(&temp, target).map_err(|error| {
            let _ = fs::remove_file(&temp);
            McpError::internal_error(
                format!("cannot replace {}: {error}", target.display()),
                None,
            )
        })
    }

    /// Validates manifest content before it reaches the plugins folder.
    ///
    /// Returns the parsed manifest so the caller can report the tiles that
    /// are about to exist. A rejected manifest is never written — the plugin
    /// keeps running on its previous, working one.
    pub(super) fn validate_manifest(
        &self,
        id: &str,
        content: &str,
        target: &Path,
    ) -> Result<PluginManifest, McpError> {
        let manifest = PluginManifest::parse(content, target)
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        if manifest.id != id {
            return Err(McpError::invalid_params(
                format!(
                    "manifest id {:?} does not match the plugin folder {id:?} — the folder name \
                     IS the plugin id; use the same value for both",
                    manifest.id
                ),
                None,
            ));
        }
        Ok(manifest)
    }

    /// Awaits the outcome of a (re)start and attaches this reload's log tail.
    pub(super) async fn settle_reload(
        &self,
        id: &str,
        watcher: StatusWatcher,
        since_ms: u64,
    ) -> ReloadOutcomeOut {
        let fallback = self
            .supervisor
            .plugin_infos()
            .into_iter()
            .find(|info| info.id == id)
            .map_or(PluginStatus::Starting, |info| info.status);
        let outcome = watcher.settle(RELOAD_TIMEOUT, fallback).await;
        let tiles = self
            .supervisor
            .plugin_infos()
            .into_iter()
            .find(|info| info.id == id)
            .and_then(|info| info.manifest)
            .map(|manifest| manifest.tiles)
            .unwrap_or_default();
        let running = outcome.status == PluginStatus::Running && outcome.settled;
        let rendered = if running {
            self.await_first_renders(id, &tiles, since_ms).await
        } else {
            Vec::new()
        };
        // Timestamp-filtered rather than tailed: the agent needs THIS reload's
        // output, not whatever the plugin logged an hour ago.
        let (entries, _skipped) = self
            .read_plugin_log(id, None, None, RELOAD_LOG_LIMIT * 20)
            .unwrap_or_default();
        let this_start: Vec<PluginLogEntry> = entries
            .into_iter()
            .filter(|entry| entry.ts >= since_ms)
            .collect();
        let warnings = summarise_warnings(&this_start);
        let logs: Vec<PluginLogEntry> = this_start
            .into_iter()
            .rev()
            .take(RELOAD_LOG_LIMIT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let design_review = running
            .then(|| design_review(id, &tiles, &warnings, &rendered))
            .flatten();
        ReloadOutcomeOut {
            status: outcome.status,
            error: outcome.error,
            settled: outcome.settled,
            hot_reload: self.supervisor.hot_reload_enabled(),
            tiles: tiles.into_iter().map(|tile| tile.id).collect(),
            logs,
            warnings,
            rendered,
            design_review,
        }
    }

    /// Waits (briefly) for every declared tile's first tile, then for the
    /// shell's report about that markup, so the reply can carry both. Returns
    /// the tiles that rendered.
    async fn await_first_renders(
        &self,
        id: &str,
        tiles: &[PluginTileDef],
        since_ms: u64,
    ) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + FIRST_RENDER_GRACE;
        let mut rendered = self.supervisor.tiles_rendered_since(id, since_ms);
        while !tiles.is_empty()
            && tiles.iter().any(|tile| !rendered.contains(&tile.id))
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(RENDER_POLL).await;
            rendered = self.supervisor.tiles_rendered_since(id, since_ms);
        }
        if !rendered.is_empty() {
            tokio::time::sleep(SHELL_REPORT_GRACE).await;
        }
        rendered
    }

    /// Restarts the plugin when nothing else will. With a working folder watch
    /// the write already triggered the reload and a second one would run the
    /// plugin twice.
    pub(super) async fn restart_if_not_watched(&self, id: &str) {
        if self.supervisor.hot_reload_enabled() {
            return;
        }
        if let Err(error) = self.supervisor.restart(id).await {
            tracing::debug!(plugin = %id, %error, "explicit restart after write failed");
        }
    }
}

impl SmabarMcp {
    /// An entry script that translates without a locale file renders raw
    /// keys; said once, into this start's warnings.
    pub(super) fn warn_missing_locales(&self, id: &str, dir: &Path, entry: &str) {
        let translates = fs::read_to_string(dir.join(entry))
            .map(|script| script.contains(".t("))
            .unwrap_or(false);
        if translates && !dir.join("locales").join("en.json").is_file() {
            crate::plugins::append_plugin_log(
                &self.paths.logs_dir(),
                id,
                "warn",
                "core",
                &format!(
                    "{entry} calls app.t() but locales/en.json does not exist — t() returns \
                     the raw key until you write locales/en.json (a flat {{\"key\": \"text\"}} \
                     map) with plugin_write_file"
                ),
                None,
            );
        }
    }
}

/// Everything this start produced that an agent must not miss: the shell's
/// markup reports, the core's rejections, and the stderr cause of a crash.
/// Distinct, source-prefixed, bounded.
fn summarise_warnings(this_start: &[PluginLogEntry]) -> ReloadWarnings {
    let mut messages: Vec<String> = Vec::new();
    for entry in this_start {
        let noteworthy = matches!(entry.source.as_str(), "shell" | "core")
            && matches!(entry.level.as_str(), "warn" | "error");
        if !noteworthy {
            continue;
        }
        let message = format!("{}: {}", entry.source, entry.message);
        if !messages.contains(&message) {
            messages.push(message);
        }
    }
    let stderr_cause = crate::plugins::stderr_cause::last_cause(
        this_start
            .iter()
            .filter(|entry| entry.source == "stderr")
            .map(|entry| entry.message.as_str()),
    );
    if let Some(cause) = stderr_cause {
        messages.push(format!("stderr: {cause}"));
    }
    let count = messages.len();
    messages.truncate(RELOAD_WARNING_LIMIT);
    for message in &mut messages {
        if message.chars().count() > RELOAD_WARNING_CHARS {
            let cut: String = message.chars().take(RELOAD_WARNING_CHARS).collect();
            *message = format!("{cut}…");
        }
    }
    ReloadWarnings { count, messages }
}

/// A contextual completion reminder with the machine-checkable criteria
/// filled in; the visual verdict stays with the agent.
fn design_review(
    id: &str,
    tiles: &[PluginTileDef],
    warnings: &ReloadWarnings,
    rendered: &[String],
) -> Option<Value> {
    if tiles.is_empty() {
        return None;
    }
    let missing: Vec<&str> = tiles
        .iter()
        .filter(|tile| !rendered.contains(&tile.id))
        .map(|tile| tile.id.as_str())
        .collect();
    let done = json!([
        {
            "criterion": "clean start: zero shell, core or stderr warnings in this start",
            "met": warnings.count == 0,
            "detail": if warnings.count == 0 {
                "0 warnings".to_string()
            } else {
                format!("{} warning(s) — see reload.warnings", warnings.count)
            }
        },
        {
            "criterion": "every declared tile rendered its tile in this start",
            "met": missing.is_empty(),
            "detail": if missing.is_empty() {
                format!("rendered: {}", rendered.join(", "))
            } else {
                format!(
                    "rendered: [{}]; missing: [{}] — render from @app.on_ready",
                    rendered.join(", "),
                    missing.join(", ")
                )
            }
        }
    ]);
    let surfaces: Vec<Value> = tiles
        .iter()
        .map(|tile| {
            let tile_id = format!("plugin:{id}:{}", tile.id);
            let mut calls = vec![json!({
                "tool": "bar_screenshot", "arguments": {"target": tile_id, "scale": 3}
            })];
            if tile.has_flyout {
                calls.push(json!({"tool": "bar_ui_state", "arguments": {
                    "action": "open_flyout", "tileId": tile_id
                }}));
                calls.push(json!({"tool": "bar_screenshot", "arguments": {
                    "target": "flyout", "scale": 2
                }}));
            }
            json!({"tileId": tile_id, "calls": calls})
        })
        .collect();
    Some(json!({
        "status": "pending",
        "instruction": "Photograph every surface with the calls below, after your last \
            write; inspect each PNG against ui_kit bestPractices (hierarchy, clipping, \
            contrast, raw locale keys, placeholder text, control size) and exercise the \
            empty, populated, editor and error states. Report the surfaces photographed \
            and any you could not. `done` lists what the host could check itself.",
        "done": done,
        "read": {"tool": "ui_kit", "arguments": {
            "sections": ["bestPractices", "snippets"]
        }},
        "tiles": surfaces,
        "logs": {"tool": "plugin_logs", "arguments": {"id": id}}
    }))
}

/// Human-readable summary of a reload outcome, for the tool's `message`.
pub(super) fn reload_message(id: &str, outcome: &ReloadOutcomeOut) -> String {
    match outcome.status {
        PluginStatus::Running if outcome.warnings.count > 0 => format!(
            "plugin \"{id}\" is running with {} warning(s) — fix them before the visual \
             review: {}",
            outcome.warnings.count,
            outcome.warnings.messages.join(" | ")
        ),
        PluginStatus::Running if outcome.design_review.is_some() => {
            let missing: Vec<&str> = outcome
                .tiles
                .iter()
                .filter(|tile| !outcome.rendered.contains(tile))
                .map(String::as_str)
                .collect();
            let render_note = if missing.is_empty() {
                format!("all {} tile(s) rendered", outcome.tiles.len())
            } else {
                format!(
                    "tile(s) {} never rendered a tile in this start — render from \
                     @app.on_ready",
                    missing.join(", ")
                )
            };
            format!(
                "plugin \"{id}\" is running — clean start ({render_note}); visual review is \
                 still pending: photograph every surface with the calls in \
                 reload.designReview before reporting completion"
            )
        }
        PluginStatus::Running => format!("plugin \"{id}\" is running — clean start"),
        PluginStatus::Failed => format!(
            "plugin \"{id}\" failed to start: {}",
            outcome.error.as_deref().unwrap_or("no reason reported")
        ),
        PluginStatus::Stopped => format!("plugin \"{id}\" is stopped"),
        PluginStatus::Deactivated => format!(
            "plugin \"{id}\" is DEACTIVATED and was not started; the files are written and \
             kept — call plugin_set_active(id=\"{id}\", active=true) to run it again"
        ),
        PluginStatus::Starting => {
            let cause = outcome
                .warnings
                .messages
                .iter()
                .find(|message| message.starts_with("stderr: "))
                .map(|message| format!("; {message} so far"))
                .unwrap_or_default();
            format!(
                "plugin \"{id}\" is still starting after {}s — a python plugin's FIRST run \
                 provisions its toolchain via uv and can take a minute; poll plugin_list or \
                 plugin_logs for the result{cause}",
                RELOAD_TIMEOUT.as_secs()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_review_only_names_surfaces_the_plugin_declares() {
        assert!(design_review("background", &[], &ReloadWarnings::default(), &[]).is_none());
        let tiles: Vec<PluginTileDef> = serde_json::from_value(json!([
            {"id": "compact", "name": "Compact"},
            {"id": "detail", "name": "Detail", "hasFlyout": true}
        ]))
        .expect("tiles");
        let warnings = ReloadWarnings::default();
        let review =
            design_review("custom", &tiles, &warnings, &["compact".to_string()]).expect("review");
        let surfaces = review["tiles"].as_array().expect("surfaces");
        assert_eq!(surfaces.len(), 2);
        assert_eq!(surfaces[0]["calls"].as_array().expect("calls").len(), 1);
        assert_eq!(surfaces[1]["calls"].as_array().expect("calls").len(), 3);
        assert_eq!(surfaces[0]["tileId"], "plugin:custom:compact");
        assert_eq!(surfaces[1]["tileId"], "plugin:custom:detail");
        // The host checks what it can: warnings and first renders.
        assert_eq!(review["done"][0]["met"], true);
        assert_eq!(review["done"][1]["met"], false);
        assert!(
            review["done"][1]["detail"]
                .as_str()
                .expect("detail")
                .contains("missing: [detail]")
        );
    }

    #[test]
    fn warnings_are_distinct_source_prefixed_and_carry_the_stderr_cause() {
        let entry = |source: &str, level: &str, message: &str| PluginLogEntry {
            ts: 1,
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
            fields: None,
        };
        let warnings = summarise_warnings(&[
            entry("log", "warn", "my own warning"),
            entry("shell", "warn", "markup removed"),
            entry("shell", "warn", "markup removed"),
            entry("core", "warn", "ui.render dropped"),
            entry("stderr", "warn", "Traceback (most recent call last):"),
            entry(
                "stderr",
                "warn",
                "ModuleNotFoundError: No module named 'views'",
            ),
        ]);
        assert_eq!(warnings.count, 3);
        assert_eq!(
            warnings.messages,
            vec![
                "shell: markup removed",
                "core: ui.render dropped",
                "stderr: ModuleNotFoundError: No module named 'views'"
            ]
        );
        assert_eq!(
            summarise_warnings(&[entry("log", "error", "mine")]).count,
            0
        );
    }
}
