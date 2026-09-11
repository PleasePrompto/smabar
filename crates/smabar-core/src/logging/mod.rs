//! JSONL file logging via `tracing`, plus a query API over the written logs.
//!
//! [`init`] writes JSON lines to daily-rotated `smabar.log.<date>` files in
//! [`SmabarPaths::logs_dir`]. The filter level comes from the `SMABAR_LOG`
//! env var (EnvFilter syntax, default `info`). Debug builds additionally log
//! compactly to stderr. [`query`] reads those files back — the basis for the
//! MCP log tools.

mod query;

use std::fs;
use std::path::PathBuf;

use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::SmabarPaths;

pub use query::{LogEntry, LogFilter, LogLevel, LogQueryResult, query};

/// Env var controlling the log filter; see [`init`].
const LOG_ENV_VAR: &str = "SMABAR_LOG";
/// Prefix of the daily-rotated JSONL log files in the logs directory.
const LOG_FILE_PREFIX: &str = "smabar.log";
/// Filter used when `SMABAR_LOG` is unset.
///
/// `rmcp=warn` is the interesting part: the MCP transport logs THREE info
/// lines per tool call ("Service initialized as server", "input stream
/// terminated", "serve finished"). An agent searching this log for a failure
/// would bury its own signal under its own search — measured at 492 of 1628
/// lines in one day. Its warnings (a rejected Origin, for one) still pass.
const DEFAULT_FILTER: &str = "info,rmcp=warn";

/// Errors from initializing logging or querying log files.
#[derive(Debug, Error)]
pub enum LoggingError {
    /// Reading or creating a log path failed.
    #[error("failed to access log path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Keeps the background log writer alive. Hold it for the lifetime of the
/// process; dropping it flushes buffered lines and stops file logging.
#[must_use = "dropping the guard stops the log writer"]
pub struct LogGuard {
    _worker: WorkerGuard,
}

/// Initialize global `tracing` output: JSONL to daily-rotated files under
/// `logs_dir()`, filtered via the `SMABAR_LOG` env var (default `info`),
/// plus a compact stderr layer in debug builds.
///
/// Records emitted through the `log` crate are captured as well, so errors
/// from Tauri and other dependencies land in the same file: `try_init`
/// installs a `LogTracer` when the `tracing-log` feature is on. The bridge
/// bounds itself — `LogTracer::enabled` rejects a record whose level is above
/// the current tracing filter before converting it — so no extra level knob
/// is needed here. `tests/logging.rs` proves both halves.
///
/// Calling this twice never panics: the second call logs a warning, keeps
/// the existing subscriber, and still returns a (then inert) guard.
pub fn init(paths: &SmabarPaths) -> Result<LogGuard, LoggingError> {
    let dir = paths.logs_dir();
    fs::create_dir_all(&dir).map_err(|source| LoggingError::Io {
        path: dir.clone(),
        source,
    })?;

    let appender = tracing_appender::rolling::daily(&dir, LOG_FILE_PREFIX);
    let (writer, worker) = tracing_appender::non_blocking(appender);

    let filter =
        EnvFilter::try_from_env(LOG_ENV_VAR).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let file_layer = fmt::layer().json().with_writer(writer);
    let stderr_layer = if cfg!(debug_assertions) {
        Some(fmt::layer().compact().with_writer(std::io::stderr))
    } else {
        None
    };

    let already_initialized = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .is_err();
    if already_initialized {
        tracing::warn!("logging already initialized; keeping the existing subscriber");
    }
    Ok(LogGuard { _worker: worker })
}
