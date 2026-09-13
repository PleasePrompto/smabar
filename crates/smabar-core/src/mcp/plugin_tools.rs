//! MCP tools for plugin CRUD and logs.

use std::fs;
use std::path::{Component, Path, PathBuf};

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::logging::{self, LogFilter, LogLevel};
use crate::plugins::MANIFEST_FILE;

use super::SmabarMcp;
use super::plugin_reload;
use super::plugin_types::{
    PluginActionParams, PluginFileOut, PluginIdParams, PluginInfoOut, PluginListResult,
    PluginLogEntry, PluginLogsParams, PluginLogsResult, PluginReadResult, PluginReloadResult,
    PluginWriteFileParams, PluginWriteResult,
};
use super::types::AckResult;
use crate::util::now_ms;

/// Files above this size are listed without content.
pub(super) const MAX_INLINE_FILE_BYTES: u64 = 64 * 1024;
const DEFAULT_LOG_LIMIT: usize = 100;
const VALID_LEVELS: &str = "trace, debug, info, warn, error";

#[tool_router(router = plugin_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    /// Every installed plugin, enriched with the two facts the supervisor
    /// does not know: where its data lives and whether a bundled update is
    /// waiting for the user. Shared by `plugin_list` and `bar_get_state`.
    pub(super) fn plugin_summaries(&self) -> Vec<PluginInfoOut> {
        let pending = crate::plugins::plugins_with_pending_update(&self.paths);
        let plugins_hidden = self.config.current().plugins_hidden;
        self.supervisor
            .plugin_infos()
            .into_iter()
            .map(|info| {
                let data_dir = self.paths.plugin_data_dir(&info.id).display().to_string();
                let update_available = pending.contains(&info.id);
                PluginInfoOut::from_info(info, data_dir, update_available, &plugins_hidden)
            })
            .collect()
    }

    #[tool(
        description = "List every installed plugin with name, version, lifecycle status, failure \
                       reason, tiles, its code folder and its writable data directory. Status \
                       is starting, running, failed, stopped, or DEACTIVATED — the last one \
                       means the user switched the plugin off (`pluginsDeactivated`): it is \
                       installed and intact but no process runs and none will, until \
                       plugin_set_active(active=true). hiddenTiles names tiles the user \
                       took off the bar while the plugin keeps running (`pluginsHidden`); \
                       plugin_set_visible(\"plugin:<id>:<tile>\", true) shows one again. \
                       updateAvailable marks a bundled plugin \
                       that was edited locally while a newer bundled version shipped — that \
                       replacement waits for the user."
    )]
    pub(super) async fn plugin_list(&self) -> Result<Json<PluginListResult>, McpError> {
        Ok(Json(PluginListResult {
            plugins: self.plugin_summaries(),
        }))
    }

    #[tool(
        description = "Read a plugin's SOURCE folder recursively: every file with size and \
                       UTF-8 content (files over 64 KiB or binary files are listed without \
                       content). Dot-entries and __pycache__ are skipped. For what the plugin \
                       WROTE at runtime — its cache, downloads, database — use plugin_data."
    )]
    pub(super) async fn plugin_read(
        &self,
        Parameters(PluginIdParams { id }): Parameters<PluginIdParams>,
    ) -> Result<Json<PluginReadResult>, McpError> {
        let dir = self.resolve_plugin_dir(&id)?;
        let mut files = Vec::new();
        collect_files(&dir, &dir, &mut files).map_err(|error| {
            McpError::internal_error(
                format!("cannot read plugin folder {}: {error}", dir.display()),
                None,
            )
        })?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Json(PluginReadResult {
            dir: dir.display().to_string(),
            files,
        }))
    }

    #[tool(
        description = "Write one whole file into a plugin folder (created as needed): `id` is \
                       the folder AND the plugin id, `path` is relative, the write is atomic. A \
                       write into a folder that has a smabar.json restarts the plugin and the \
                       reply waits for the outcome: reload.status (running | failed | starting \
                       | stopped | deactivated), reload.error, reload.settled, reload.tiles, \
                       reload.rendered (tiles whose tile appeared in this start), \
                       reload.warnings (every shell markup report, core rejection and stderr \
                       cause of this start — count 0 is a clean start), reload.logs (the newest \
                       20 entries) and, after a clean start with tiles, reload.designReview \
                       with the exact photograph calls and the host-checked `done` criteria. A \
                       smabar.json is validated before anything is written (id equals the \
                       folder; a python entry script must already exist in the folder); a \
                       rejected manifest changes nothing and a running plugin survives it. New \
                       plugin: siblings first (views.py, locales/en.json, plugin.py), \
                       smabar.json LAST — the manifest write is the start. Gotcha: status \
                       running with warnings.count above 0 is a broken plugin, not a clean \
                       start."
    )]
    pub(super) async fn plugin_write_file(
        &self,
        Parameters(PluginWriteFileParams { id, path, content }): Parameters<PluginWriteFileParams>,
    ) -> Result<Json<PluginWriteResult>, McpError> {
        validate_plugin_id(&id)?;
        let rel = sanitize_rel_path(&path)?;
        let dir = self.paths.plugins_dir().join(&id);
        let target = dir.join(&rel);
        let is_manifest = rel.as_os_str() == MANIFEST_FILE;
        let manifest = if is_manifest {
            let manifest = self.validate_manifest(&id, &content, &target)?;
            // The one start failure a manifest predicts: no entry script. A
            // refused write keeps the folder as it was, so a running plugin
            // survives, and the agent gets the fix instead of a crash.
            if let Some(error) = crate::plugins::manifest_start_error(&dir, &manifest) {
                return Err(McpError::invalid_params(
                    format!("{MANIFEST_FILE} was NOT written: {error}"),
                    None,
                ));
            }
            Some(manifest)
        } else {
            None
        };
        // Subscribe BEFORE the write: the write is what triggers the reload,
        // and a broadcast receiver only sees events sent after it subscribed.
        let watcher = self.supervisor.watch_status(&id);
        let since_ms = now_ms();
        if let Some(entry) = manifest
            .as_ref()
            .and_then(|manifest| manifest.entry.as_deref())
        {
            self.warn_missing_locales(&id, &dir, entry);
        }
        self.atomic_write(&target, &content)?;

        // Only a folder that HAS a manifest can start something. Awaiting the
        // entry-script write of a new plugin would just burn the timeout.
        if !dir.join(MANIFEST_FILE).is_file() {
            return Ok(Json(PluginWriteResult {
                path: rel.display().to_string(),
                bytes_written: content.len(),
                message: format!(
                    "wrote {}; sibling stored, no {MANIFEST_FILE} in this folder yet — write \
                     the manifest LAST, that write starts the plugin and its reply is the \
                     test result",
                    target.display()
                ),
                reload: None,
            }));
        }
        self.restart_if_not_watched(&id).await;
        let outcome = self.settle_reload(&id, watcher, since_ms).await;
        let message = format!(
            "wrote {}; {}",
            target.display(),
            plugin_reload::reload_message(&id, &outcome)
        );
        Ok(Json(PluginWriteResult {
            path: rel.display().to_string(),
            bytes_written: content.len(),
            message,
            reload: Some(outcome),
        }))
    }

    #[tool(
        description = "Stop and restart a plugin process, then report the real outcome: \
                       status, failure reason, the tile ids it declares, this start's \
                       warnings, rendered tiles and log lines. Also starts a plugin whose folder exists but failed \
                       to load before (e.g. after fixing its manifest). A DEACTIVATED plugin is \
                       refused, not silently skipped — switch it on with \
                       plugin_set_active(active=true) first."
    )]
    pub(super) async fn plugin_reload(
        &self,
        Parameters(PluginIdParams { id }): Parameters<PluginIdParams>,
    ) -> Result<Json<PluginReloadResult>, McpError> {
        validate_plugin_id(&id)?;
        let watcher = self.supervisor.watch_status(&id);
        let since_ms = now_ms();
        self.supervisor
            .restart(&id)
            .await
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let outcome = self.settle_reload(&id, watcher, since_ms).await;
        Ok(Json(PluginReloadResult {
            message: plugin_reload::reload_message(&id, &outcome),
            reload: outcome,
        }))
    }

    #[tool(
        description = "Queue one UI action for a RUNNING plugin through the same handler as a \
                       data-action click, form submit, or custom context-menu entry. `id` comes \
                       from plugin_list; `tile` is the manifest-local tile id (for example \
                       `weather`, NOT `plugin:weather:weather`); `action` must match the \
                       plugin's @app.on_action registration. Optional `value` is delivered as \
                       arbitrary JSON. Success means the event was QUEUED, not that the \
                       handler or its next render has completed; take the follow-up screenshot \
                       after the plugin publishes its new UI. Unknown or stopped plugins are \
                       rejected — use plugin_list to inspect their id and status."
    )]
    pub(super) async fn plugin_action(
        &self,
        Parameters(PluginActionParams {
            id,
            tile,
            action,
            value,
        }): Parameters<PluginActionParams>,
    ) -> Result<Json<AckResult>, McpError> {
        validate_plugin_id(&id)?;
        self.supervisor
            .dispatch_action(&id, &tile, &action, value)
            .await
            .map_err(|error| {
                McpError::invalid_params(
                    format!("{error}; use plugin_list to check the plugin id and running status"),
                    None,
                )
            })?;
        Ok(Json(AckResult {
            message: format!("queued action \"{action}\" for plugin \"{id}\" tile \"{tile}\""),
        }))
    }

    #[tool(
        description = "Why did that not work — one call. With `id`: `pluginEntries` from the \
                       process, each with a `source`: \"log\" (app.log), \"stdout\" (printed \
                       non-RPC lines), \"stderr\" (tracebacks), \"core\" (a request smabar \
                       rejected or an input it ignored, with the supported alternative), \
                       \"shell\" (markup the sanitizer removed, an unknown sb-* class, inline \
                       style, an unnamed icon-only control, a raw locale key — with the fix; \
                       once per distinct problem per surface, at most 32 distinct problems per \
                       surface, re-armed by a clean render); and \
                       `coreEntries`, what smabar said about the plugin (started, crashed, \
                       restart backoff, invalid manifest). A plugin that dies before its first \
                       line has only coreEntries. Without `id`: the bar's own log (target \
                       smabar::shell for UI errors) and its dependencies. Filters: `level` \
                       minimum (trace|debug|info|warn|error), `contains` substring, `limit` \
                       newest N (default 100). Gotcha: a clean start is silence — read entries \
                       whose ts is newer than your last write; older entries describe a \
                       previous version."
    )]
    pub(super) async fn plugin_logs(
        &self,
        Parameters(params): Parameters<PluginLogsParams>,
    ) -> Result<Json<PluginLogsResult>, McpError> {
        let min_level = params
            .level
            .as_deref()
            .map(|raw| {
                LogLevel::parse(raw).ok_or_else(|| {
                    McpError::invalid_params(
                        format!("unknown level \"{raw}\"; valid levels: {VALID_LEVELS}"),
                        None,
                    )
                })
            })
            .transpose()?;
        let limit = params.limit.unwrap_or(DEFAULT_LOG_LIMIT);

        match params.id {
            Some(id) => {
                validate_plugin_id(&id)?;
                let (entries, skipped_lines) =
                    self.read_plugin_log(&id, min_level, params.contains.as_deref(), limit)?;
                // The plugin's own file is only half the story: a plugin that
                // never starts writes nothing into it, and everything the
                // supervisor knows about the failure lives in the core log.
                // Returning both makes one call enough to diagnose a plugin.
                let core = logging::query(
                    &self.paths,
                    &LogFilter {
                        min_level,
                        substring: params.contains.clone(),
                        plugin: Some(id.clone()),
                        limit: Some(limit),
                    },
                )
                .map_err(|error| {
                    McpError::internal_error(format!("cannot query core logs: {error}"), None)
                })?;
                Ok(Json(PluginLogsResult {
                    plugin_entries: Some(entries),
                    core_entries: Some(core.entries),
                    skipped_lines: skipped_lines + core.skipped_lines,
                }))
            }
            None => {
                let filter = LogFilter {
                    min_level,
                    substring: params.contains,
                    plugin: None,
                    limit: Some(limit),
                };
                let result = logging::query(&self.paths, &filter).map_err(|error| {
                    McpError::internal_error(format!("cannot query core logs: {error}"), None)
                })?;
                Ok(Json(PluginLogsResult {
                    plugin_entries: None,
                    core_entries: Some(result.entries),
                    skipped_lines: result.skipped_lines,
                }))
            }
        }
    }
}

impl SmabarMcp {
    /// Resolves a plugin id to its folder via the supervisor's own resolver,
    /// so the tools and the Tauri commands never disagree about which folder
    /// an id means.
    pub(super) fn resolve_plugin_dir(&self, id: &str) -> Result<PathBuf, McpError> {
        validate_plugin_id(id)?;
        self.supervisor.plugin_dir(id).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "no plugin \"{id}\": not registered and no folder {} exists",
                    self.paths.plugins_dir().join(id).display()
                ),
                None,
            )
        })
    }

    /// Parses `logs/plugin-<id>.log`, newest `limit` entries after filtering.
    /// A missing log file yields an empty result.
    pub(super) fn read_plugin_log(
        &self,
        id: &str,
        min_level: Option<LogLevel>,
        contains: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<PluginLogEntry>, usize), McpError> {
        let path = self.paths.logs_dir().join(format!("plugin-{id}.log"));
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), 0));
            }
            Err(error) => {
                return Err(McpError::internal_error(
                    format!("cannot read {}: {error}", path.display()),
                    None,
                ));
            }
        };
        let mut entries = Vec::new();
        let mut skipped = 0usize;
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: PluginLogEntry = match serde_json::from_str(line) {
                Ok(entry) => entry,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if let Some(min) = min_level {
                // Unknown level strings fail the comparison and are dropped.
                match LogLevel::parse(&entry.level) {
                    Some(level) if level >= min => {}
                    _ => continue,
                }
            }
            if let Some(needle) = contains
                && !line.contains(needle)
            {
                continue;
            }
            entries.push(entry);
        }
        if entries.len() > limit {
            entries.drain(..entries.len() - limit);
        }
        Ok((entries, skipped))
    }
}

/// Enforces the manifest id charset so ids are always safe path segments.
pub(super) fn validate_plugin_id(id: &str) -> Result<(), McpError> {
    if crate::plugins::is_valid_plugin_id(id) {
        Ok(())
    } else {
        Err(McpError::invalid_params(
            format!("plugin id \"{id}\" must be non-empty and contain only [a-z0-9-]"),
            None,
        ))
    }
}

/// Rebuilds `raw` from its plain path components; anything that could escape
/// the plugin folder (`..`, absolute paths, drive prefixes) is rejected.
pub(super) fn sanitize_rel_path(raw: &str) -> Result<PathBuf, McpError> {
    let mut clean = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(McpError::invalid_params(
                    format!(
                        "path \"{raw}\" must stay inside the plugin folder: no \"..\", \
                         absolute paths, or drive prefixes"
                    ),
                    None,
                ));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(McpError::invalid_params(
            "path must name a file inside the plugin folder".to_string(),
            None,
        ));
    }
    Ok(clean)
}

/// Recursively collects the files under `dir` (skipping dot-entries and
/// `__pycache__`), with content for UTF-8 files up to the inline limit.
fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PluginFileOut>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "__pycache__" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }
        let size = entry.metadata()?.len();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let (content, truncated, binary) = if size > MAX_INLINE_FILE_BYTES {
            (None, true, false)
        } else {
            match String::from_utf8(fs::read(&path)?) {
                Ok(text) => (Some(text), false, false),
                Err(_) => (None, false, true),
            }
        };
        files.push(PluginFileOut {
            path: rel,
            size,
            content,
            truncated,
            binary,
        });
    }
    Ok(())
}
