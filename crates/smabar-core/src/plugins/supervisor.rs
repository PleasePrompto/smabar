//! The supervisor itself: startup scan, hot-reload watcher, settings fan-out,
//! and the public API.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use notify::RecommendedWatcher;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::activation::{self, DeactivatedMap};
use super::await_status::StatusWatcher;
pub(super) use super::lifecycle_guard::lifecycle_guard;
use super::logfile::PluginDiagnostics;
use super::manifest::{MANIFEST_FILE, PluginManifest, PluginRuntime};
use super::provision::{self, RuntimeProvisioner};
use super::runner::{PluginCommand, RunCtx, run_lifecycle};
use super::status::{self, StatusMap, UiMap};
use super::watcher::{config_change_loop, dir_change_loop, list_plugin_dirs, spawn_dir_watcher};
use super::{PluginError, PluginEvent, PluginStatus, SupervisorOptions};
use crate::util::lock_unpoisoned;

const EVENT_CHANNEL_CAPACITY: usize = 64;
const COMMAND_CHANNEL_CAPACITY: usize = 64;
const ACTION_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a plugin may take to shut down before its task is aborted.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// A registered plugin: the channel to its lifecycle task plus bookkeeping.
pub(super) struct PluginHandle {
    pub(super) dir: PathBuf,
    pub(super) manifest: PluginManifest,
    pub(super) commands: mpsc::Sender<PluginCommand>,
    task: JoinHandle<()>,
}

pub(super) struct Inner {
    pub(super) sessions: Arc<super::commands::Sessions>,
    pub(super) paths: SmabarPaths,
    hub: ProviderHub,
    pub(super) config: Arc<ConfigWatcher>,
    options: SupervisorOptions,
    pub(super) events: broadcast::Sender<PluginEvent>,
    /// Running (or finally-failed) plugins, keyed by manifest id.
    pub(super) plugins: Mutex<HashMap<String, PluginHandle>>,
    /// Serializes every asynchronous stop/start transition per plugin id.
    pub(super) lifecycle_locks: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    /// Blocks shutdown until every in-flight lifecycle transition has left.
    pub(super) lifecycle_barrier: tokio::sync::RwLock<()>,
    /// Terminal gate shared by watcher, config and runtime-ready tasks.
    pub(super) shutdown: CancellationToken,
    /// Installed plugins the user switched off (see [`super::activation`]).
    /// They have no process but stay visible, so they can be switched on.
    pub(super) deactivated: Mutex<DeactivatedMap>,
    /// Latest status per plugin id (or folder name), fed by
    /// [`status::track_events`]. Shared as its own `Arc` so the tracking
    /// task does not keep `Inner` (and thus the event sender) alive.
    pub(super) statuses: StatusMap,
    /// The one managed-Python-runtime state; see [`provision`].
    pub(super) runtime: RuntimeProvisioner,
    /// Process-lifetime dedupe for actionable malformed-input warnings.
    pub(super) diagnostics: PluginDiagnostics,
    /// Last rendered HTML per (plugin, tile, target), fed by the same
    /// tracking task; late/reloaded shells fetch it via `current_ui`.
    pub(super) ui: UiMap,
    /// Keeps the plugins-dir watch alive; `None` when watching failed
    /// (plugins then only load at startup).
    pub(super) watcher: Mutex<Option<RecommendedWatcher>>,
}

/// Supervises all installed plugins; see the [module docs](super).
/// Cloning shares the same supervisor state (cheap `Arc` clone).
pub struct PluginSupervisor {
    pub(super) inner: Arc<Inner>,
    background_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Clone for PluginSupervisor {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            background_tasks: Arc::clone(&self.background_tasks),
        }
    }
}

impl PluginSupervisor {
    pub fn capabilities(&self) -> &'static [&'static str] {
        self.inner.options.capabilities()
    }

    /// Scans the plugins directory, starts every folder with a manifest, and
    /// begins watching for folder and settings changes. Must be called inside
    /// a tokio runtime.
    ///
    /// Never fails: unusable plugin folders surface as `failed` status
    /// events, a broken directory watch as a log warning (no hot reload).
    pub async fn start(
        paths: SmabarPaths,
        hub: ProviderHub,
        config: Arc<ConfigWatcher>,
        options: SupervisorOptions,
    ) -> Self {
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let plugins_dir = paths.plugins_dir();
        if let Err(error) = fs::create_dir_all(&plugins_dir) {
            tracing::warn!(%error, path = %plugins_dir.display(), "cannot create plugins directory");
        }
        let (dir_tx, dir_rx) = mpsc::unbounded_channel();
        let watcher = spawn_dir_watcher(&plugins_dir, dir_tx);
        let statuses: StatusMap = Arc::new(Mutex::new(HashMap::new()));
        let ui: UiMap = Arc::new(Mutex::new(HashMap::new()));
        // Subscribe before the first plugin starts so no event is lost.
        let status_task = tokio::spawn(status::track_events(
            events.subscribe(),
            Arc::clone(&statuses),
            Arc::clone(&ui),
        ));
        let runtime = RuntimeProvisioner::new(paths.tools_dir(), options.uv_override.clone());
        let diagnostics = PluginDiagnostics::new(paths.logs_dir());
        let inner = Arc::new(Inner {
            sessions: Arc::new(super::commands::Sessions::default()),
            paths,
            hub,
            config,
            options,
            events,
            plugins: Mutex::new(HashMap::new()),
            lifecycle_locks: Mutex::new(HashMap::new()),
            lifecycle_barrier: tokio::sync::RwLock::new(()),
            shutdown: CancellationToken::new(),
            deactivated: Mutex::new(DeactivatedMap::new()),
            statuses,
            runtime,
            diagnostics,
            ui,
            watcher: Mutex::new(watcher),
        });
        let runtime_task = tokio::spawn(provision::nudge_on_ready(
            Arc::downgrade(&inner),
            inner.runtime.subscribe(),
        ));

        // Subscribe before the synchronous boot scan. A config write during
        // that scan must be queued instead of leaving the just-started
        // processes on a stale activation/settings snapshot.
        let config_rx = inner.config.subscribe();
        let applied_config = inner.config.current();

        // `start_plugin` itself refuses a deactivated plugin, so the boot
        // scan needs no filter of its own — and cannot forget one.
        for dir in list_plugin_dirs(&plugins_dir) {
            start_plugin(&inner, &dir);
        }
        let directory_task = tokio::spawn(dir_change_loop(Arc::clone(&inner), dir_rx));
        let config_task = tokio::spawn(config_change_loop(
            Arc::clone(&inner),
            config_rx,
            applied_config,
        ));
        Self {
            inner,
            background_tasks: Arc::new(Mutex::new(vec![
                status_task,
                runtime_task,
                directory_task,
                config_task,
            ])),
        }
    }

    /// Subscribes to plugin lifecycle and UI events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<PluginEvent> {
        self.inner.events.subscribe()
    }

    /// Handle onto the managed-Python-runtime state (status, events, retry).
    pub fn runtime(&self) -> RuntimeProvisioner {
        self.inner.runtime.clone()
    }

    /// Subscribes to `plugin_id`'s next lifecycle outcome.
    ///
    /// Call this BEFORE the action that triggers the (re)start — a broadcast
    /// receiver only sees what is sent after it subscribes.
    pub fn watch_status(&self, plugin_id: &str) -> StatusWatcher {
        StatusWatcher::new(self.inner.events.subscribe(), plugin_id)
    }

    /// Whether plugin folders are watched. `false` means the OS watch could
    /// not be created at startup: files still get written, but nothing
    /// reloads by itself — callers have to restart the plugin explicitly.
    pub fn hot_reload_enabled(&self) -> bool {
        lock_unpoisoned(&self.inner.watcher).is_some()
    }

    /// Stops and restarts a registered plugin. An unregistered id whose
    /// folder contains a manifest (e.g. just fixed after a failed load) is
    /// started fresh instead.
    ///
    /// A deactivated plugin is refused rather than silently doing nothing:
    /// the caller asked for a running process and would otherwise be told
    /// the restart succeeded.
    pub async fn restart(&self, plugin_id: &str) -> Result<(), PluginError> {
        let Some((_transition, _lifecycle)) = lifecycle_guard(&self.inner, plugin_id).await else {
            return Err(PluginError::NotRunning {
                id: plugin_id.to_string(),
            });
        };
        if activation::is_deactivated(&self.inner, plugin_id) {
            return Err(PluginError::Deactivated {
                id: plugin_id.to_string(),
            });
        }
        let handle = lock_unpoisoned(&self.inner.plugins).remove(plugin_id);
        if let Some(handle) = handle {
            let dir = handle.dir.clone();
            tracing::info!(plugin = %plugin_id, "restart requested; stopping current instance");
            stop_handle(handle).await;
            start_plugin(&self.inner, &dir);
            return Ok(());
        }
        let dir = self.inner.paths.plugins_dir().join(plugin_id);
        if dir.join(MANIFEST_FILE).is_file() {
            start_plugin(&self.inner, &dir);
            return Ok(());
        }
        Err(PluginError::UnknownPlugin {
            id: plugin_id.to_string(),
        })
    }

    /// Forwards a tile interaction to its plugin as an `event` notification.
    pub async fn dispatch_action(
        &self,
        plugin_id: &str,
        tile_id: &str,
        action: &str,
        value: Option<Value>,
    ) -> Result<(), PluginError> {
        let commands = {
            let plugins = lock_unpoisoned(&self.inner.plugins);
            let handle = plugins
                .get(plugin_id)
                .ok_or_else(|| PluginError::UnknownPlugin {
                    id: plugin_id.to_string(),
                })?;
            if !handle.manifest.tiles.iter().any(|tile| tile.id == tile_id) {
                return Err(PluginError::UnknownTile {
                    id: plugin_id.to_string(),
                    tile_id: tile_id.to_string(),
                    available: handle
                        .manifest
                        .tiles
                        .iter()
                        .map(|tile| tile.id.clone())
                        .collect(),
                });
            }
            handle.commands.clone()
        };
        let (accepted, received) = oneshot::channel();
        commands
            .try_send(PluginCommand::Action {
                tile_id: tile_id.to_string(),
                action: action.to_string(),
                value,
                accepted,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => PluginError::Busy {
                    id: plugin_id.to_string(),
                },
                mpsc::error::TrySendError::Closed(_) => PluginError::NotRunning {
                    id: plugin_id.to_string(),
                },
            })?;
        tokio::time::timeout(ACTION_TIMEOUT, received)
            .await
            .map_err(|_| PluginError::Busy {
                id: plugin_id.to_string(),
            })?
            .map_err(|_| PluginError::NotRunning {
                id: plugin_id.to_string(),
            })
    }

    /// Gracefully stops all plugins (shutdown request → grace → kill), in
    /// parallel, and waits until they are gone.
    pub async fn shutdown_all(&self) {
        self.inner.shutdown.cancel();
        // Stop producing folder events before waiting for an in-flight
        // transition. Config/runtime loops observe the same cancellation.
        lock_unpoisoned(&self.inner.watcher).take();
        let _transition = self.inner.lifecycle_barrier.write().await;
        let handles: Vec<PluginHandle> = lock_unpoisoned(&self.inner.plugins)
            .drain()
            .map(|(_, handle)| handle)
            .collect();
        let mut stops = JoinSet::new();
        for handle in handles {
            stops.spawn(stop_handle(handle));
        }
        while stops.join_next().await.is_some() {}

        let tasks = std::mem::take(&mut *lock_unpoisoned(&self.background_tasks));
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            if let Err(error) = task.await
                && !error.is_cancelled()
            {
                tracing::warn!(%error, "plugin supervisor task failed during shutdown; restart smabar before managing plugins again");
            }
        }
    }
}

/// Registers and launches the plugin in `dir`. Invalid manifests and
/// duplicate ids surface as failed-status events.
pub(super) fn start_plugin(inner: &Arc<Inner>, dir: &Path) {
    if inner.shutdown.is_cancelled() {
        return;
    }
    let folder = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    let manifest = match PluginManifest::load(dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::warn!(plugin = %folder, %error, "plugin manifest rejected");
            emit_from_task(
                inner,
                PluginEvent::Status {
                    plugin_id: folder,
                    status: PluginStatus::Failed,
                    error: Some(error.to_string()),
                },
            );
            return;
        }
    };
    start_loaded_plugin(inner, dir, manifest);
}

/// Registers a manifest that was parsed while the previous instance was
/// still alive. The watcher uses this to avoid a parse-after-stop race.
pub(super) fn start_loaded_plugin(inner: &Arc<Inner>, dir: &Path, manifest: PluginManifest) {
    if inner.shutdown.is_cancelled() {
        return;
    }
    let folder = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    // Agents write plugin files one at a time and the watcher fires on the
    // first write: a python plugin whose entry script has not arrived yet
    // must not enter the spawn/backoff loop (5 noisy "no such file" runs).
    // The watcher re-runs this folder on every file change, so the failure
    // heals itself the moment the entry file is written. Exec runtimes are
    // not checked — command[0] may live on PATH.
    if let Some(error) = manifest_start_error(dir, &manifest) {
        tracing::warn!(plugin = %manifest.id, %error, "plugin not started");
        emit_from_task(
            inner,
            PluginEvent::Status {
                plugin_id: manifest.id,
                status: PluginStatus::Failed,
                error: Some(error),
            },
        );
        return;
    }
    // The one gate for every start there is — boot scan, folder watcher,
    // explicit restart — so a switched-off plugin cannot be resurrected by
    // any of them, least of all by a write to its own folder.
    if activation::gate(inner, dir, &manifest) {
        return;
    }
    let mut plugins = lock_unpoisoned(&inner.plugins);
    if let Some(existing) = plugins.get(&manifest.id)
        && existing.dir.as_path() != dir
    {
        let error = format!(
            "duplicate plugin id \"{}\": already provided by {}",
            manifest.id,
            existing.dir.display()
        );
        tracing::warn!(plugin = %folder, %error, "plugin rejected");
        emit_from_task(
            inner,
            PluginEvent::Status {
                plugin_id: folder,
                status: PluginStatus::Failed,
                error: Some(error),
            },
        );
        return;
    }
    inner.diagnostics.warn_new_items(
        &manifest.id,
        "manifest",
        manifest.diagnostics(),
        PluginManifest::format_diagnostic,
    );
    tracing::info!(plugin = %manifest.id, path = %dir.display(), "starting plugin");
    let (commands, commands_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let ctx = RunCtx {
        sessions: inner.sessions.clone(),
        dir: dir.to_path_buf(),
        manifest: manifest.clone(),
        paths: inner.paths.clone(),
        hub: inner.hub.clone(),
        config: Arc::clone(&inner.config),
        options: inner.options.clone(),
        events: inner.events.clone(),
        runtime: inner.runtime.clone(),
        diagnostics: inner.diagnostics.clone(),
    };
    let task = tokio::spawn(run_lifecycle(ctx, commands_rx));
    plugins.insert(
        manifest.id.clone(),
        PluginHandle {
            dir: dir.to_path_buf(),
            manifest,
            commands,
            task,
        },
    );
}

/// The one start error a manifest alone can predict: a python entry script
/// that is not there. Shared by the boot scan, the folder watcher and the
/// MCP write gate, so every path reports the same fix.
pub(crate) fn manifest_start_error(dir: &Path, manifest: &PluginManifest) -> Option<String> {
    let entry = (manifest.runtime == PluginRuntime::Python)
        .then_some(manifest.entry.as_deref())
        .flatten()?;
    (!dir.join(entry).is_file()).then(|| {
        format!(
            "entry file \"{entry}\" does not exist in the plugin folder — write it with \
             plugin_write_file(path=\"{entry}\") first, then smabar.json LAST"
        )
    })
}

/// Emits an event from a spawned task, so a subscriber attaching right after
/// [`PluginSupervisor::start`] (before yielding to the runtime) misses nothing.
fn emit_from_task(inner: &Arc<Inner>, event: PluginEvent) {
    let events = inner.events.clone();
    tokio::spawn(async move {
        let _ = events.send(event);
    });
}

/// Asks the lifecycle task to shut down and waits; aborts it after
/// [`STOP_TIMEOUT`] (the child process is then killed via kill_on_drop).
pub(super) async fn stop_handle(handle: PluginHandle) {
    stop_handle_with_timeout(handle, STOP_TIMEOUT).await;
}

async fn stop_handle_with_timeout(handle: PluginHandle, timeout: Duration) {
    let commands = handle.commands;
    let mut task = handle.task;
    let stopped = tokio::time::timeout(timeout, async {
        // Err = lifecycle already ended (final failure); nothing left to stop.
        let _ = commands.send(PluginCommand::Shutdown).await;
        let _ = (&mut task).await;
    })
    .await;
    if stopped.is_err() {
        tracing::warn!("plugin lifecycle task did not stop in time; aborting it");
        task.abort();
        // `abort` only schedules cancellation. Joining is what proves the
        // child/process guards were dropped before a restart or deletion.
        let _ = task.await;
    }
}

#[cfg(test)]
#[path = "supervisor_stop_tests.rs"]
mod stop_tests;
