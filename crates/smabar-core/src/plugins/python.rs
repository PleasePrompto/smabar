//! Let uv maintain script environments, then run their Python without a resident uv.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use thiserror::Error;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::{ChildStderr, Command};

use super::SupervisorOptions;
use super::logfile::PluginLog;
use super::process::{SpawnError, spawn_command};
use super::rpc::{BoundedLine, read_bounded_line};

/// A Python path is a single short line, not an unbounded command response.
const MAX_INTERPRETER_OUTPUT: u64 = 16 * 1024;

#[derive(Debug, Error)]
pub(super) enum PrepareError {
    #[error("Python environment preparation failed: {0}")]
    Failed(String),
    #[error("could not {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot prepend the Python environment to PATH: {0}")]
    Path(#[from] std::env::JoinPathsError),
}

pub(super) async fn command(
    uv: &Path,
    entry: &str,
    dir: &Path,
    data_dir: &Path,
    plugin_id: &str,
    options: &SupervisorOptions,
    log: &mut PluginLog,
) -> Result<Command, SpawnError> {
    let script = tokio::fs::File::open(dir.join(entry))
        .await
        .map_err(|source| PrepareError::Io {
            operation: "read the Python plugin entry",
            source,
        })?;
    // Only detect the tag. uv owns PEP 723 parsing and validation. Plain
    // scripts retain uv run's interpreter/project discovery semantics.
    if !has_metadata(script).await? || std::env::var_os("UV_NO_CACHE").is_some() {
        return Ok(uv_run(uv, entry));
    }
    let uv_command = |args: &[&str]| {
        let mut command = Command::new(uv);
        command
            .args(args)
            .current_dir(dir)
            .env("SMABAR_PLUGIN_ID", plugin_id)
            .env("SMABAR_PLUGIN_DIR", dir)
            .env("SMABAR_PLUGIN_DATA_DIR", data_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = &options.python_install_dir {
            command.env("UV_PYTHON_INSTALL_DIR", path);
        }
        if let Some(path) = &options.sdk_path {
            command.env("PYTHONPATH", super::process::python_path_with(path));
        }
        command
    };
    // Like uv run, keep unrelated installed packages. With no script lockfile,
    // uv sync resolves directly and does not create one in the watched code dir.
    output(uv_command(&["sync", "--inexact", "--script", entry]), log).await?;
    let path = output(uv_command(&["python", "find", "--script", entry]), log).await?;
    let path = std::str::from_utf8(&path)
        .map_err(|_| PrepareError::Failed("uv returned a non-UTF-8 interpreter path".into()))?;
    let path = PathBuf::from(path.trim_end_matches(['\r', '\n']));
    // uv config `no-cache = true` syncs into a temporary environment and find
    // then answers with the base interpreter: such plugins keep running the
    // way they did before, under a resident uv.
    let Some((scripts, environment)) = script_environment(&path) else {
        log.write(
            "warn",
            "core",
            "uv keeps no persistent script environment (uv no-cache?); starting through uv run --script with a resident uv process",
            None,
        );
        tracing::warn!(
            plugin = plugin_id,
            interpreter = %path.display(),
            "no persistent script environment; falling back to uv run"
        );
        return Ok(uv_run(uv, entry));
    };
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let search_path = std::env::join_paths(
        std::iter::once(scripts.to_path_buf()).chain(std::env::split_paths(&inherited)),
    )
    .map_err(PrepareError::from)?;
    let mut command = Command::new(&path);
    command
        .arg(entry)
        .env("VIRTUAL_ENV", environment)
        .env("PATH", search_path);
    tracing::debug!(
        plugin = plugin_id,
        "Python script environment prepared; starting interpreter directly"
    );
    Ok(command)
}

fn uv_run(uv: &Path, entry: &str) -> Command {
    let mut command = Command::new(uv);
    command.args(["run", "--script", entry]);
    command
}

/// The `bin`/`Scripts` directory and the root of the virtual environment
/// `python` belongs to, if it is an interpreter inside one.
fn script_environment(python: &Path) -> Option<(&Path, &Path)> {
    let scripts = python
        .parent()
        .filter(|_| python.is_absolute() && python.is_file())?;
    let environment = scripts
        .parent()
        .filter(|root| root.join("pyvenv.cfg").is_file())?;
    Some((scripts, environment))
}

async fn output(command: Command, log: &mut PluginLog) -> Result<Vec<u8>, SpawnError> {
    let mut process = spawn_command(command)?;
    let stdout = process
        .child
        .stdout
        .take()
        .ok_or_else(|| PrepareError::Failed("uv stdout pipe is missing".into()))?;
    let stderr = process
        .child
        .stderr
        .take()
        .ok_or_else(|| PrepareError::Failed("uv stderr pipe is missing".into()))?;
    let read_output = async move {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_INTERPRETER_OUTPUT + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > MAX_INTERPRETER_OUTPUT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "uv output exceeded 16 KiB",
            ));
        }
        Ok(bytes)
    };
    let (bytes, (), status) =
        tokio::try_join!(read_output, log_stderr(stderr, log), process.child.wait(),).map_err(
            |source| PrepareError::Io {
                operation: "prepare the Python environment with uv",
                source,
            },
        )?;
    if !status.success() {
        return Err(PrepareError::Failed(log.explain_failure(format!(
            "uv exited with {status}; check the plugin dependencies and network, then restart it"
        )))
        .into());
    }
    Ok(bytes)
}

async fn has_metadata(script: tokio::fs::File) -> Result<bool, PrepareError> {
    let mut reader = BufReader::new(script);
    let mut buffer = Vec::new();
    loop {
        match read_bounded_line(&mut reader, &mut buffer)
            .await
            .map_err(|source| PrepareError::Io {
                operation: "read Python script metadata",
                source,
            })? {
            BoundedLine::Line(line) if line == "# /// script" => return Ok(true),
            BoundedLine::Eof => return Ok(false),
            _ => {}
        }
    }
}

async fn log_stderr(stderr: ChildStderr, log: &mut PluginLog) -> std::io::Result<()> {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    loop {
        match read_bounded_line(&mut reader, &mut buffer).await? {
            BoundedLine::Eof => return Ok(()),
            BoundedLine::Line(line) if line.trim().is_empty() => {}
            BoundedLine::Line(line) => log.write("info", "stderr", &line, None),
            BoundedLine::TooLong => log.write(
                "warn",
                "core",
                "uv stderr line exceeded the plugin log line limit and was discarded",
                None,
            ),
        }
    }
}
