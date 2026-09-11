//! Per-plugin supervision: spawn → initialize → serve → shutdown/crash,
//! wrapped in a restart loop with exponential backoff.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::backoff::{MAX_CONSECUTIVE_FAILURES, STABLE_RUN, restart_delay};
use super::handlers;
use super::logfile::{PluginDiagnostics, PluginLog};
use super::manifest::{PluginManifest, PluginRuntime};
use super::provision::{RuntimeProvisioner, RuntimeStatus};
use crate::platform::ProcessSignal;

use super::process::{PluginProcess, spawn_plugin};
use super::rpc::{
    CHANNEL_CAPACITY, Incoming, MAX_LINE_BYTES, PendingMap, RpcClient, spawn_stderr_reader,
    spawn_stdout_reader, spawn_writer,
};
use super::{PluginEvent, PluginStatus, SupervisorOptions};

/// initialize keeps a generous timeout: the runtime is provisioned before
/// the spawn (see [`RuntimeProvisioner`]), but the first `uv run` of a plugin
/// may still resolve its PEP 723 dependencies over the network.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PING_TIMEOUT: Duration = Duration::from_secs(10);
/// Grace both for the shutdown request and for the voluntary exit after it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);
/// Wait for the OS to reap a killed process.
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
/// Extra grace after SIGTERM to the process group, before SIGKILL.
const GROUP_TERM_GRACE: Duration = Duration::from_secs(1);
/// After the process is gone, give the stdio readers a moment to reach EOF so
/// the final captured output can be drained into the plugin log.
const READER_DRAIN: Duration = Duration::from_secs(1);

/// Commands from the supervisor to one plugin's lifecycle task.
#[derive(Debug)]
pub(crate) enum PluginCommand {
    /// Forward a tile interaction as an `event` notification.
    Action {
        tile_id: String,
        action: String,
        value: Option<Value>,
        accepted: oneshot::Sender<()>,
    },
    /// Push new settings as a `settings.changed` notification.
    SettingsChanged { settings: Value },
    /// Stop the plugin gracefully and end the lifecycle task.
    Shutdown,
}

/// Everything one plugin's lifecycle task needs.
pub(crate) struct RunCtx {
    pub sessions: Arc<super::commands::Sessions>,
    pub dir: PathBuf,
    pub manifest: PluginManifest,
    pub paths: SmabarPaths,
    pub hub: ProviderHub,
    pub config: Arc<ConfigWatcher>,
    pub options: SupervisorOptions,
    pub events: broadcast::Sender<PluginEvent>,
    pub runtime: RuntimeProvisioner,
    pub diagnostics: PluginDiagnostics,
}

impl RunCtx {
    /// The writable directory this plugin owns — never the code folder.
    pub(crate) fn data_dir(&self) -> PathBuf {
        self.paths.plugin_data_dir(&self.manifest.id)
    }

    pub(crate) fn emit(&self, event: PluginEvent) {
        // Err just means nobody is subscribed right now.
        let _ = self.events.send(event);
    }

    fn emit_status(&self, status: PluginStatus, error: Option<String>) {
        self.emit(PluginEvent::Status {
            plugin_id: self.manifest.id.clone(),
            status,
            error,
        });
    }

    /// This plugin's persisted settings (empty object when absent).
    pub(crate) fn current_settings(&self) -> Value {
        self.config
            .current()
            .plugins
            .get(&self.manifest.id)
            .cloned()
            .unwrap_or_else(|| json!({}))
    }
}

/// How one run of the plugin process ended.
enum RunEnd {
    /// Graceful shutdown requested and completed; do not restart.
    Shutdown,
    /// The plugin died or failed to start; restart with backoff.
    Failure(String),
}

/// Restart loop around [`run_once`]: emits `Added` once, then supervises the
/// process until shutdown or the consecutive-failure limit.
pub(crate) async fn run_lifecycle(ctx: RunCtx, mut commands: mpsc::Receiver<PluginCommand>) {
    ctx.emit(PluginEvent::Added {
        plugin_id: ctx.manifest.id.clone(),
        name: ctx.manifest.name.clone(),
        icon_data_url: ctx.manifest.icon_data_url.clone(),
        tiles: ctx.manifest.tiles.clone(),
        settings_schema: ctx.manifest.settings_schema.clone(),
    });
    let mut log = PluginLog::open(&ctx.paths.logs_dir(), &ctx.manifest.id);
    let mut failures: u32 = 0;
    loop {
        ctx.emit_status(PluginStatus::Starting, None);
        let started = Instant::now();
        match run_once(&ctx, &mut commands, &mut log).await {
            RunEnd::Shutdown => {
                ctx.emit_status(PluginStatus::Stopped, None);
                tracing::info!(plugin = %ctx.manifest.id, "plugin stopped");
                return;
            }
            RunEnd::Failure(reason) => {
                if started.elapsed() >= STABLE_RUN {
                    failures = 0;
                }
                failures += 1;
                tracing::warn!(plugin = %ctx.manifest.id, failures, %reason, "plugin run failed");
                if failures >= MAX_CONSECUTIVE_FAILURES {
                    ctx.emit_status(
                        PluginStatus::Failed,
                        Some(format!(
                            "{reason} — {failures} consecutive failures, \
                             giving up until the plugin folder changes"
                        )),
                    );
                    return;
                }
                ctx.emit_status(PluginStatus::Failed, Some(reason));
                let delay = restart_delay(failures);
                tracing::info!(
                    plugin = %ctx.manifest.id,
                    delay_secs = delay.as_secs(),
                    "restarting plugin after backoff"
                );
                if !backoff_wait(&mut commands, delay).await {
                    ctx.emit_status(PluginStatus::Stopped, None);
                    return;
                }
            }
        }
    }
}

/// Sleeps the backoff delay while staying responsive to `Shutdown`. Returns
/// `false` when the plugin should stop instead of restarting.
async fn backoff_wait(commands: &mut mpsc::Receiver<PluginCommand>, delay: Duration) -> bool {
    let deadline = Instant::now() + delay;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return true,
            command = commands.recv() => match command {
                Some(PluginCommand::Action { accepted, .. }) => {
                    // Dropping the ack tells the caller the down process never
                    // accepted this action; it must not receive false success.
                    drop(accepted);
                }
                Some(PluginCommand::SettingsChanged { .. }) => {
                    // Initialize delivers the latest settings on restart.
                }
                Some(PluginCommand::Shutdown) | None => return false,
            },
        }
    }
}

/// One spawn-to-exit run of the plugin process.
async fn run_once(
    ctx: &RunCtx,
    commands: &mut mpsc::Receiver<PluginCommand>,
    log: &mut PluginLog,
) -> RunEnd {
    // A python plugin needs the managed runtime first; a Failed outcome feeds
    // the normal backoff, and once the runtime installs, the provisioner's
    // Ready event revives plugins that parked while it was missing.
    if ctx.manifest.runtime == PluginRuntime::Python
        && let RuntimeStatus::Failed { message, .. } = ctx.runtime.ensure().await
    {
        return RunEnd::Failure(format!("python runtime unavailable: {message}"));
    }
    let data_dir = ctx.data_dir();
    // The plugin may write here from its very first handler, so the directory
    // has to exist before the process does.
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        return RunEnd::Failure(format!(
            "cannot create the plugin data directory {}: {error}",
            data_dir.display()
        ));
    }
    log.begin_run();
    let mut process = match spawn_plugin(&ctx.manifest, &ctx.dir, &data_dir, &ctx.options) {
        Ok(process) => process,
        Err(error) => return RunEnd::Failure(error.to_string()),
    };
    let child = &mut process.child;
    let (stdin, stdout, stderr) =
        match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
            (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
            _ => {
                kill_and_reap(&mut process).await;
                return RunEnd::Failure("plugin process is missing a stdio pipe".to_string());
            }
        };

    let pending = PendingMap::default();
    let (out_tx, out_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (in_tx, mut in_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let writer = spawn_writer(stdin, out_rx);
    let stdout_reader = spawn_stdout_reader(stdout, pending.clone(), in_tx.clone());
    let stderr_reader = spawn_stderr_reader(stderr, in_tx);
    let rpc = RpcClient::new(out_tx, pending);

    let mut forwarders: HashMap<_, JoinHandle<()>> = HashMap::new();
    let end = serve(
        ctx,
        &rpc,
        &mut process,
        commands,
        &mut in_rx,
        log,
        &mut forwarders,
    )
    .await;

    for forwarder in forwarders.into_values() {
        forwarder.abort();
    }
    // Whatever ended the run, make sure the process is gone; on the graceful
    // path this start_kill hits an already-exited child and is a no-op.
    kill_and_reap(&mut process).await;
    // The dead process's pipes hit EOF — let the readers finish naturally
    // (dropping the handles on timeout is fine, EOF ends them anyway), then
    // drain what they captured. The output of a fast-dying process is the
    // prime debugging material and must reach the plugin log.
    let _ = tokio::time::timeout(READER_DRAIN, async {
        let _ = stdout_reader.await;
        let _ = stderr_reader.await;
    })
    .await;
    while let Ok(message) = in_rx.try_recv() {
        match message {
            Incoming::Stdout(line) => log.write("info", "stdout", &line, None),
            Incoming::Stderr(line) => log.write("warn", "stderr", &line, None),
            Incoming::LineTooLong { source } => warn_line_too_long(ctx, source),
            _ => {}
        }
    }
    writer.abort();
    // The traceback drained above is the reason an agent needs; a bare
    // "plugin closed stdout" only says that the process is gone.
    match end {
        RunEnd::Failure(reason) => RunEnd::Failure(log.explain_failure(reason)),
        RunEnd::Shutdown => RunEnd::Shutdown,
    }
}

async fn desktop_request(
    ctx: &RunCtx,
    session: &super::commands::Session,
    method: &str,
    params: &Value,
) -> Result<Value, String> {
    let port = ctx
        .options
        .host
        .as_ref()
        .ok_or("desktop services unavailable")?;
    let owner = super::HostSession {
        plugin_id: ctx.manifest.id.clone(),
        generation: session.generation,
        plugin_dir: ctx.dir.clone(),
        data_dir: ctx.data_dir(),
        tiles: ctx.manifest.tiles.iter().map(|w| w.id.clone()).collect(),
        stopped: session.stopped.clone(),
        rpc: session.rpc.clone(),
    };
    port.request(owner, method, params).await
}

/// initialize handshake plus the serving loop of a healthy plugin.
async fn serve(
    ctx: &RunCtx,
    rpc: &RpcClient,
    process: &mut PluginProcess,
    commands: &mut mpsc::Receiver<PluginCommand>,
    incoming: &mut mpsc::Receiver<Incoming>,
    log: &mut PluginLog,
    forwarders: &mut HashMap<crate::providers::ProviderKind, JoinHandle<()>>,
) -> RunEnd {
    let init_settings = ctx.current_settings();
    let language = ctx.config.current().language;
    let params = json!({
        "protocolVersion": 1,
        "pluginId": ctx.manifest.id,
        // dataDir is WRITABLE and outside the watched code folder; pluginDir
        // is the read-only code folder (locales live there).
        "dataDir": ctx.data_dir().display().to_string(),
        "pluginDir": ctx.dir.display().to_string(),
        "settings": init_settings.clone(),
        "language": language,
        "locale": crate::i18n::resolve(&ctx.paths, &language),
        "providers": ctx.hub.available_names(),
        "capabilities": ctx.options.capabilities(),
    });
    let reply = match rpc.request("initialize", params, INITIALIZE_TIMEOUT).await {
        Ok(reply) => reply,
        Err(error) => return RunEnd::Failure(format!("initialize failed: {error}")),
    };
    let commands_info = match super::commands::parse_commands(&reply) {
        Ok(commands) => commands,
        Err(error) => return RunEnd::Failure(error),
    };
    let session = ctx
        .sessions
        .enter(&ctx.manifest.id, rpc.clone(), commands_info);
    ctx.emit_status(PluginStatus::Running, None);
    tracing::info!(plugin = %ctx.manifest.id, "plugin running");
    // The config may have changed between the initialize snapshot above and
    // this point (settings_set while the handshake was in flight, or a push
    // that was dropped while this run was starting). Deliver the newest
    // settings once so the plugin never keeps serving a stale state.
    let latest_settings = ctx.current_settings();
    if latest_settings != init_settings {
        tracing::debug!(
            plugin = %ctx.manifest.id,
            "settings changed during initialize; pushing settings.changed"
        );
        let _ = rpc
            .notify("settings.changed", json!({"settings": latest_settings}))
            .await;
    }

    let mut ping = tokio::time::interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(PluginCommand::Action { tile_id, action, value, accepted }) => {
                    let mut params = json!({"tileId": tile_id, "action": action});
                    if let Some(value) = value {
                        params["value"] = value;
                    }
                    if rpc.notify("event", params).await {
                        let _ = accepted.send(());
                    }
                }
                Some(PluginCommand::SettingsChanged { settings }) => {
                    let _ = rpc.notify("settings.changed", json!({"settings": settings})).await;
                }
                Some(PluginCommand::Shutdown) | None => {
                    graceful_shutdown(ctx, rpc, process).await;
                    return RunEnd::Shutdown;
                }
            },
            message = incoming.recv() => match message {
                Some(Incoming::Notification { method, params }) => {
                    handlers::handle_notification(ctx, log, &method, &params);
                }
                Some(Incoming::Request { id, method, params }) => {
                    if (method.starts_with("audio.") || method.starts_with("ui.popup.")) && ctx.options.host.is_some() {
                        let result = desktop_request(ctx, &session.session, &method, &params).await;
                        match result {
                            Ok(value) => rpc.respond_ok(&id, value).await,
                            Err(message) => {
                                ctx.diagnostics.warn_once(&ctx.manifest.id, &message, None);
                                rpc.respond_error(&id, -32000, &message).await;
                            }
                        }
                    } else {
                        handlers::handle_request(ctx, rpc, forwarders, &id, &method, &params).await;
                    }
                }
                Some(Incoming::Stdout(line)) => log.write("info", "stdout", &line, None),
                Some(Incoming::Stderr(line)) => log.write("warn", "stderr", &line, None),
                Some(Incoming::LineTooLong { source }) => warn_line_too_long(ctx, source),
                Some(Incoming::StdoutClosed) | None => {
                    return RunEnd::Failure(
                        "plugin closed stdout (process exit or crash)".to_string(),
                    );
                }
            },
            _ = ping.tick() => {
                if let Err(error) = rpc.request("ping", json!({}), PING_TIMEOUT).await {
                    return RunEnd::Failure(format!("ping failed: {error}"));
                }
            }
        }
    }
}

fn warn_line_too_long(ctx: &RunCtx, source: &str) {
    ctx.diagnostics.warn_once(
        &ctx.manifest.id,
        &format!(
            "plugin {source} line exceeded {MAX_LINE_BYTES} bytes and was discarded; emit one UTF-8 JSON-RPC message or log entry per line below that limit"
        ),
        None,
    );
}

/// shutdown request → up to 3 s for a voluntary exit → SIGTERM to the whole
/// process group → up to 1 s more; the caller's [`kill_and_reap`] then
/// finishes off stragglers with SIGKILL.
///
/// The group step matters because the plugin is not alone: a python plugin
/// runs under `uv`, and a plugin may spawn CLI children of its own. Signalling
/// the group first gives every one of them the chance to exit on its own.
async fn graceful_shutdown(ctx: &RunCtx, rpc: &RpcClient, process: &mut PluginProcess) {
    if let Err(error) = rpc.request("shutdown", json!({}), SHUTDOWN_GRACE).await {
        tracing::debug!(plugin = %ctx.manifest.id, %error, "graceful shutdown request failed");
    }
    if tokio::time::timeout(SHUTDOWN_GRACE, process.child.wait())
        .await
        .is_ok()
    {
        // The plugin itself is gone; its children may not be.
        process.signal_group(ProcessSignal::Terminate);
        return;
    }
    tracing::warn!(plugin = %ctx.manifest.id, "plugin ignored the shutdown request; terminating its process group");
    process.signal_group(ProcessSignal::Terminate);
    let _ = tokio::time::timeout(GROUP_TERM_GRACE, process.child.wait()).await;
}

/// Force-kills the plugin's whole process group (no-op when already gone) and
/// reaps the direct child.
///
/// The group signal goes FIRST and is not conditional on the child still
/// running: when the child was already reaped, killing it is a no-op while its
/// grandchildren are exactly what would otherwise be left orphaned.
async fn kill_and_reap(process: &mut PluginProcess) {
    process.signal_group(ProcessSignal::Kill);
    // start_kill errors when the child was already reaped — fine.
    let _ = process.child.start_kill();
    if tokio::time::timeout(REAP_TIMEOUT, process.child.wait())
        .await
        .is_err()
    {
        tracing::warn!("plugin process did not exit after kill");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn backoff_rejects_instead_of_acknowledging_an_action() {
        let (commands, mut receiver) = mpsc::channel(2);
        let (accepted, result) = oneshot::channel();
        commands
            .send(PluginCommand::Action {
                tile_id: "status".into(),
                action: "refresh".into(),
                value: None,
                accepted,
            })
            .await
            .expect("queue action");
        commands
            .send(PluginCommand::Shutdown)
            .await
            .expect("queue shutdown");

        assert!(!backoff_wait(&mut receiver, Duration::from_secs(60)).await);
        assert!(
            result.await.is_err(),
            "a down process must not accept actions"
        );
    }
}
