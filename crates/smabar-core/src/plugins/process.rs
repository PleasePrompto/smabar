//! Building and spawning plugin child processes.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use thiserror::Error;
use tokio::process::{Child, Command};

use crate::platform::{ProcessSignal, signal_process_group};

use super::SupervisorOptions;
use super::logfile::PluginLog;
use super::manifest::{PluginManifest, PluginRuntime};

/// A spawned plugin: the child handle plus the process group it leads.
pub(crate) struct PluginProcess {
    pub(crate) child: Child,
    /// `None` on platforms without process groups; never derived from
    /// anything but this child's own pid, so signalling it can only ever
    /// reach the plugin's own tree.
    group: Option<u32>,
}

impl PluginProcess {
    /// Signals the plugin's whole process tree. Returns whether the group
    /// was still there to receive it.
    pub(crate) fn signal_group(&self, signal: ProcessSignal) -> bool {
        self.group
            .is_some_and(|pgid| signal_process_group(pgid, signal))
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        // Also runs when startup is cancelled: uv build subprocesses must not
        // outlive the plugin. Child's kill_on_drop kills and reaps the leader.
        self.signal_group(ProcessSignal::Kill);
    }
}

/// Errors while preparing or spawning a plugin process.
#[derive(Debug, Error)]
pub(crate) enum SpawnError {
    /// No usable `uv` for the python runtime.
    #[error(
        "uv executable not found in PATH (required for python plugins); \
         install uv (https://docs.astral.sh/uv/) or set SupervisorOptions::uv_override"
    )]
    UvNotFound,
    /// Manifest validation guarantees entry/command; this is the defensive
    /// error if that invariant is ever violated.
    #[error("manifest is missing the {0} for its runtime")]
    MissingLaunchInfo(&'static str),
    #[error(transparent)]
    Python(#[from] super::python::PrepareError),
    /// The OS refused to start the process.
    #[error("failed to spawn {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// Spawns the plugin process described by `manifest`: piped stdio, cwd set to
/// the plugin folder, killed if the handle is dropped.
///
/// `dir` is the immutable code folder (`SMABAR_PLUGIN_DIR`), `data_dir` the
/// writable one the plugin owns (`SMABAR_PLUGIN_DATA_DIR`). They are
/// deliberately different: the supervisor watches `dir` and restarts on every
/// write inside it.
///
/// Returns the child plus its process-group id, which is `None` on platforms
/// without process groups. The plugin leads its own group so stopping it can
/// take its whole tree down, including commands started by the plugin itself.
pub(crate) async fn spawn_plugin(
    manifest: &PluginManifest,
    dir: &Path,
    data_dir: &Path,
    options: &SupervisorOptions,
    log: &mut PluginLog,
) -> Result<PluginProcess, SpawnError> {
    let mut command = base_command(manifest, dir, data_dir, options, log).await?;
    command
        .current_dir(dir)
        .env("SMABAR_PLUGIN_ID", &manifest.id)
        .env("SMABAR_PLUGIN_DIR", dir)
        .env("SMABAR_PLUGIN_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn_command(command)
}

/// Shared by plugin execution and its short-lived environment preparation.
pub(super) fn spawn_command(mut command: Command) -> Result<PluginProcess, SpawnError> {
    let grouped = crate::platform::detach_into_own_process_group(command.as_std_mut());
    crate::platform::configure_no_window(command.as_std_mut());
    command.kill_on_drop(true);
    crate::platform::render::scrub_child_env(command.as_std_mut());
    let program = command
        .as_std()
        .get_program()
        .to_string_lossy()
        .into_owned();
    let child = command
        .spawn()
        .map_err(|source| SpawnError::Spawn { program, source })?;
    // Read the pid NOW: `Child::id()` returns None once the child is awaited,
    // which is exactly when the group still needs signalling.
    let group = grouped.then(|| child.id()).flatten();
    Ok(PluginProcess { child, group })
}

async fn base_command(
    manifest: &PluginManifest,
    dir: &Path,
    data_dir: &Path,
    options: &SupervisorOptions,
    log: &mut PluginLog,
) -> Result<Command, SpawnError> {
    match manifest.runtime {
        PluginRuntime::Exec => {
            let (program, args) = manifest
                .command
                .split_first()
                .ok_or(SpawnError::MissingLaunchInfo("command"))?;
            let mut command = Command::new(program);
            command.args(args);
            Ok(command)
        }
        PluginRuntime::Python => {
            let entry = manifest
                .entry
                .as_deref()
                .ok_or(SpawnError::MissingLaunchInfo("entry"))?;
            let uv = resolve_uv(options.uv_override.as_deref())?;
            let mut command =
                super::python::command(&uv, entry, dir, data_dir, &manifest.id, options, log)
                    .await?;
            if let Some(sdk_path) = &options.sdk_path {
                command.env("PYTHONPATH", python_path_with(sdk_path));
            }
            if let Some(python_install_dir) = &options.python_install_dir {
                command.env("UV_PYTHON_INSTALL_DIR", python_install_dir);
            }
            Ok(command)
        }
    }
}

/// `uv_override` if set, otherwise the first `uv` found on `PATH`. Shared
/// with the runtime provisioner so both resolve uv identically.
pub(super) fn resolve_uv(uv_override: Option<&Path>) -> Result<PathBuf, SpawnError> {
    if let Some(uv) = uv_override {
        return Ok(uv.to_path_buf());
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let binary = format!("uv{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&path)
        .map(|dir| dir.join(&binary))
        .find(|candidate| candidate.is_file())
        .ok_or(SpawnError::UvNotFound)
}

/// The SDK path, followed by any inherited `PYTHONPATH` entries.
pub(super) fn python_path_with(sdk_path: &Path) -> std::ffi::OsString {
    let mut parts = vec![sdk_path.to_path_buf()];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        parts.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(parts).unwrap_or_else(|error| {
        tracing::warn!(%error, "cannot join PYTHONPATH entries; using only the SDK path");
        sdk_path.as_os_str().to_os_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn python_command_sets_managed_python_install_dir() {
        let manifest: PluginManifest = serde_json::from_str(
            r#"{"id":"x","name":"X","version":"1","protocolVersion":1,
                "runtime":"python","entry":"plugin.py",
                "tiles":[{"id":"w","name":"W"}]}"#,
        )
        .expect("parse manifest");
        let options = SupervisorOptions {
            uv_override: Some(PathBuf::from("/tools/uv")),
            python_install_dir: Some(PathBuf::from("/home/test/.smabar/tools")),
            ..SupervisorOptions::default()
        };

        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("plugin.py"), "pass\n").expect("script");
        let mut log = PluginLog::open(dir.path(), "x");
        let command = base_command(&manifest, dir.path(), dir.path(), &options, &mut log)
            .await
            .expect("build python command");
        let install_dir = command
            .as_std()
            .get_envs()
            .find(|(key, _)| *key == "UV_PYTHON_INSTALL_DIR")
            .and_then(|(_, value)| value);
        assert_eq!(
            install_dir,
            Some(std::ffi::OsStr::new("/home/test/.smabar/tools"))
        );
    }
}

#[cfg(all(test, unix))]
mod group_tests {
    use super::*;

    #[tokio::test]
    async fn a_spawned_plugin_leads_its_own_process_group() {
        let manifest: PluginManifest = serde_json::from_str(
            r#"{"id":"g","name":"G","version":"1","protocolVersion":1,
                "runtime":"exec","command":["sleep","5"],
                "tiles":[{"id":"w","name":"W"}]}"#,
        )
        .expect("parse manifest");
        let dir = tempfile::tempdir().expect("temp dir");
        let mut log = PluginLog::open(dir.path(), "g");
        let process = spawn_plugin(
            &manifest,
            dir.path(),
            dir.path(),
            &SupervisorOptions::default(),
            &mut log,
        )
        .await
        .expect("spawn");
        assert!(
            process.group.is_some(),
            "without a group id nothing can reach the plugin's children"
        );
        assert_eq!(process.group, process.child.id());
        // The id is only a GROUP id if the child actually leads a new group.
        let pid = process.child.id().expect("pid") as i32;
        // SAFETY: getpgid on a live child pid.
        let pgid = unsafe { libc::getpgid(pid) };
        assert_eq!(pgid, pid, "the plugin must lead its own process group");
    }
}
