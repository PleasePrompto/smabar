//! Managed Python runtime provisioning: an explicit, observable
//! `uv python install` instead of the invisible one inside the first
//! script environment preparation.
//!
//! The provisioner is the single authority on the runtime's state. Python
//! plugins wait in [`await_runtime`] before spawning, the headless
//! `smabar --provision` entrypoint calls [`RuntimeProvisioner::ensure`] from
//! the installer, and the shell's retry button calls it again after a
//! failure. Concurrent callers coalesce on one install attempt — its outcome,
//! Ready or Failed, is shared instead of repeated; uv itself locks the
//! install directory, so a second smabar process (the installer hook racing
//! the app) at worst runs one redundant, short-lived "already installed"
//! command. An interrupted download needs no bookkeeping either: nothing
//! marks the attempt, the probe still reports the runtime absent, and the
//! next trigger re-runs the idempotent install.
//!
//! A missing, downloading or failed runtime is a WAIT for a python plugin,
//! never a plugin failure: the plugin reports `starting` with the reason and
//! retries on the [`runtime_retry_delay`] cadence, so a first start without
//! network recovers by itself once the network is back.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc};

use super::PluginStatus;
use super::backoff::runtime_retry_delay;
use super::process::resolve_uv;
use super::runner::{PluginCommand, RunCtx, backoff_wait};
use crate::util::lock_unpoisoned;

/// The managed interpreter line smabar provisions. The bundled plugins and
/// the SDK require `>=3.12` in their PEP 723 headers, which 3.14 satisfies.
pub const PYTHON_VERSION: &str = "3.14";

const EVENT_CHANNEL_CAPACITY: usize = 16;
/// Hard ceiling for one `uv python install` run, download included.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
/// How many trailing output lines feed a failure message.
const STDERR_TAIL_LINES: usize = 8;

/// Why provisioning failed, so the shell can offer an actionable message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeFailureKind {
    /// The download could not reach the network.
    Offline,
    /// No uv binary — the bundled sidecar is missing or SMABAR_UV is wrong.
    UvMissing,
    /// Anything else; retry stays available.
    Other,
}

/// Lifecycle of the managed Python runtime, serialized straight to the shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuntimeStatus {
    /// No managed interpreter and no install running (also the state after an
    /// interrupted install — see the module docs).
    Absent,
    /// `uv python install` is running; `detail` is its last output line.
    Installing {
        detail: Option<String>,
    },
    Ready,
    Failed {
        message: String,
        kind: RuntimeFailureKind,
    },
}

/// Whether `tools_dir` already holds a managed interpreter: a
/// `cpython-<version>*` directory containing `bin/python3` (Unix layout) or
/// `python.exe` (Windows layout) — both are checked so no platform `cfg` is
/// needed. A false negative only costs a redundant idempotent install; a
/// false positive is healed by uv's own script-environment provisioning.
pub(crate) fn runtime_present(tools_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(tools_dir) else {
        return false;
    };
    let prefix = format!("cpython-{PYTHON_VERSION}");
    entries.flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with(&prefix)
            && (entry.path().join("bin/python3").is_file()
                || entry.path().join("python.exe").is_file())
    })
}

/// Maps uv's error text onto a failure kind. Display wording is uv's and may
/// drift across versions; anything unmatched degrades to [`Other`], which
/// still offers a retry.
///
/// [`Other`]: RuntimeFailureKind::Other
fn classify_failure(stderr_tail: &str) -> RuntimeFailureKind {
    const OFFLINE_MARKERS: [&str; 6] = [
        "error sending request",
        "connection refused",
        "timed out",
        "dns error",
        "network",
        "failed to fetch",
    ];
    let lower = stderr_tail.to_lowercase();
    if OFFLINE_MARKERS.iter().any(|marker| lower.contains(marker)) {
        RuntimeFailureKind::Offline
    } else {
        RuntimeFailureKind::Other
    }
}

struct ProvisionerInner {
    tools_dir: PathBuf,
    uv_override: Option<PathBuf>,
    status: Mutex<RuntimeStatus>,
    /// Status transitions only; subscribers render, they never decide.
    events: broadcast::Sender<RuntimeStatus>,
    /// Coalesces concurrent [`RuntimeProvisioner::ensure`] calls.
    install: tokio::sync::Mutex<()>,
    /// Completed install attempts. A caller that queued behind the lock
    /// while an attempt finished shares that attempt's outcome instead of
    /// running uv again — six plugins booting offline cost one uv run.
    attempts: AtomicU64,
}

/// Cheap-clone handle onto the one runtime state (same shape as
/// [`super::PluginSupervisor`]).
pub struct RuntimeProvisioner {
    inner: Arc<ProvisionerInner>,
}

impl Clone for RuntimeProvisioner {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl RuntimeProvisioner {
    /// Probes `tools_dir` for the initial status (Ready or Absent) without
    /// spawning anything and without emitting an event.
    pub fn new(tools_dir: PathBuf, uv_override: Option<PathBuf>) -> Self {
        let initial = if runtime_present(&tools_dir) {
            RuntimeStatus::Ready
        } else {
            RuntimeStatus::Absent
        };
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            inner: Arc::new(ProvisionerInner {
                tools_dir,
                uv_override,
                status: Mutex::new(initial),
                events,
                install: tokio::sync::Mutex::new(()),
                attempts: AtomicU64::new(0),
            }),
        }
    }

    pub fn status(&self) -> RuntimeStatus {
        lock_unpoisoned(&self.inner.status).clone()
    }

    /// Subscribes to status transitions. The boot probe never emits, so the
    /// first `Ready` on this bus always means an install just finished.
    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeStatus> {
        self.inner.events.subscribe()
    }

    fn transition(&self, status: RuntimeStatus) {
        *lock_unpoisoned(&self.inner.status) = status.clone();
        // Err just means nobody is subscribed right now.
        let _ = self.inner.events.send(status);
    }

    /// Makes sure the managed runtime exists: fast path when Ready, otherwise
    /// one install run shared by all concurrent callers — whichever way it
    /// ends. A Failed outcome poisons nothing: the next call after it tries
    /// again.
    pub async fn ensure(&self) -> RuntimeStatus {
        if self.status() == RuntimeStatus::Ready {
            return RuntimeStatus::Ready;
        }
        let seen = self.inner.attempts.load(Ordering::Acquire);
        let _guard = self.inner.install.lock().await;
        if self.status() == RuntimeStatus::Ready
            || self.inner.attempts.load(Ordering::Acquire) != seen
        {
            // A concurrent caller finished an attempt while we waited; its
            // outcome is this call's answer.
            return self.status();
        }
        let outcome = self.install().await;
        self.transition(outcome.clone());
        self.inner.attempts.fetch_add(1, Ordering::AcqRel);
        outcome
    }

    async fn install(&self) -> RuntimeStatus {
        let uv = match resolve_uv(self.inner.uv_override.as_deref()) {
            Ok(uv) => uv,
            Err(error) => {
                return RuntimeStatus::Failed {
                    message: error.to_string(),
                    kind: RuntimeFailureKind::UvMissing,
                };
            }
        };
        tracing::info!(
            python = PYTHON_VERSION,
            dir = %self.inner.tools_dir.display(),
            "provisioning managed python runtime"
        );
        self.transition(RuntimeStatus::Installing { detail: None });

        let mut command = Command::new(uv);
        command
            .arg("python")
            .arg("install")
            .arg(PYTHON_VERSION)
            .env("UV_PYTHON_INSTALL_DIR", &self.inner.tools_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::platform::configure_no_window(command.as_std_mut());
        crate::platform::render::scrub_child_env(command.as_std_mut());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return RuntimeStatus::Failed {
                    message: format!("failed to spawn uv: {error}"),
                    kind: RuntimeFailureKind::Other,
                };
            }
        };

        // uv writes progress and errors to stderr, one discrete line each on
        // a non-tty. Every line becomes the Installing detail and feeds the
        // failure tail.
        let tail: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let reader = child.stderr.take().map(|stderr| {
            let provisioner = self.clone();
            let tail = Arc::clone(&tail);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    tracing::debug!(line = %line, "uv python install");
                    let mut tail = lock_unpoisoned(&tail);
                    if tail.len() >= STDERR_TAIL_LINES {
                        tail.remove(0);
                    }
                    tail.push(line.clone());
                    drop(tail);
                    provisioner.transition(RuntimeStatus::Installing { detail: Some(line) });
                }
            })
        });

        let status = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                return RuntimeStatus::Failed {
                    message: format!("failed to wait for uv: {error}"),
                    kind: RuntimeFailureKind::Other,
                };
            }
            Err(_) => {
                let _ = child.kill().await;
                return RuntimeStatus::Failed {
                    message: format!(
                        "uv python install did not finish within {} minutes",
                        INSTALL_TIMEOUT.as_secs() / 60
                    ),
                    kind: RuntimeFailureKind::Other,
                };
            }
        };
        if let Some(reader) = reader {
            // The child exited, so the reader is at (or a moment from) EOF.
            let _ = reader.await;
        }
        if status.success() {
            tracing::info!(python = PYTHON_VERSION, "managed python runtime ready");
            return RuntimeStatus::Ready;
        }
        let message = lock_unpoisoned(&tail).join("\n");
        let message = if message.is_empty() {
            format!("uv python install exited with {status}")
        } else {
            message
        };
        let kind = classify_failure(&message);
        tracing::warn!(%message, ?kind, "python provisioning failed");
        RuntimeStatus::Failed { message, kind }
    }
}

/// What a waiting python plugin reports as its `starting` reason.
fn wait_reason(status: &RuntimeStatus) -> String {
    match status {
        RuntimeStatus::Failed { message, .. } => {
            format!("waiting for the Python runtime: {message}")
        }
        _ => "waiting for the Python runtime: downloading".to_string(),
    }
}

/// Holds a python plugin's lifecycle until the managed runtime is Ready.
///
/// Every round is one shared [`RuntimeProvisioner::ensure`]; a failed round
/// reports the reason as `starting` (never `failed`, never a consecutive
/// failure) and sleeps [`runtime_retry_delay`], waking early when the bus
/// says Ready — the timer, not the bus, guarantees the next round, so a
/// lost or lagged Ready costs at most one delay. Returns `false` when the
/// plugin was told to shut down meanwhile.
pub(super) async fn await_runtime(
    ctx: &RunCtx,
    commands: &mut mpsc::Receiver<PluginCommand>,
) -> bool {
    let mut attempts: u32 = 0;
    loop {
        // Subscribe before ensure so a Ready produced by another caller
        // during this round is not missed by the wait below.
        let mut updates = ctx.runtime.subscribe();
        if !matches!(
            ctx.runtime.status(),
            RuntimeStatus::Ready | RuntimeStatus::Failed { .. }
        ) {
            ctx.emit_status(
                PluginStatus::Starting,
                Some(wait_reason(&RuntimeStatus::Absent)),
            );
        }
        let status = ctx.runtime.ensure().await;
        if status == RuntimeStatus::Ready {
            return true;
        }
        attempts += 1;
        let delay = runtime_retry_delay(attempts);
        tracing::info!(
            plugin = %ctx.manifest.id,
            attempts,
            delay_secs = delay.as_secs(),
            "python runtime unavailable; plugin waits for the next install attempt"
        );
        ctx.emit_status(PluginStatus::Starting, Some(wait_reason(&status)));
        if !runtime_wait(commands, &mut updates, delay).await {
            return false;
        }
    }
}

/// The shutdown-responsive backoff sleep, cut short by a Ready on the bus.
async fn runtime_wait(
    commands: &mut mpsc::Receiver<PluginCommand>,
    updates: &mut broadcast::Receiver<RuntimeStatus>,
    delay: Duration,
) -> bool {
    let mut sleep = std::pin::pin!(backoff_wait(commands, delay));
    loop {
        tokio::select! {
            keep_going = &mut sleep => return keep_going,
            update = updates.recv() => match update {
                Ok(RuntimeStatus::Ready) => return true,
                Ok(_) => {}
                // Lagged: the Ready may be among the dropped events; the
                // next round re-checks the level. Closed cannot happen while
                // the context holds the provisioner, and would only mean the
                // same.
                Err(_) => return true,
            },
        }
    }
}

#[cfg(test)]
#[path = "provision_unit_tests.rs"]
mod tests;
