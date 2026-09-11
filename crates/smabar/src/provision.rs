//! Headless `smabar --provision`: warms up the managed Python runtime so the
//! Windows installer can pre-provision right after install — without starting
//! the bar, the single-instance plugin, or any window.
//!
//! Windows release builds have `windows_subsystem = "windows"`, so there is
//! no console to print to: the tracing log under `~/.smabar/logs/` IS the
//! output, and the exit code is the machine-readable result.

use std::path::PathBuf;

use smabar_core::config::SmabarPaths;
use smabar_core::plugins::{RuntimeFailureKind, RuntimeProvisioner, RuntimeStatus};

/// Runs one provisioning pass to completion and maps it to an exit code.
pub fn run(paths: &SmabarPaths, uv_override: Option<PathBuf>) -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "cannot start a tokio runtime for --provision");
            return 1;
        }
    };
    let provisioner = RuntimeProvisioner::new(paths.tools_dir(), uv_override);
    let status = runtime.block_on(provisioner.ensure());
    match &status {
        RuntimeStatus::Ready => {
            tracing::info!("managed python runtime is ready");
        }
        RuntimeStatus::Failed { message, kind } => {
            tracing::error!(%message, ?kind, "provisioning failed");
        }
        other => tracing::error!(?other, "provisioning ended in an unexpected state"),
    }
    exit_code(&status)
}

/// 0 = ready (freshly installed or already present), 2 = no uv binary,
/// 1 = anything else. The NSIS hook ignores the code; humans and scripts
/// read it.
fn exit_code(status: &RuntimeStatus) -> i32 {
    match status {
        RuntimeStatus::Ready => 0,
        RuntimeStatus::Failed {
            kind: RuntimeFailureKind::UvMissing,
            ..
        } => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_distinguish_ready_uv_missing_and_failure() {
        assert_eq!(exit_code(&RuntimeStatus::Ready), 0);
        assert_eq!(
            exit_code(&RuntimeStatus::Failed {
                message: "no uv".into(),
                kind: RuntimeFailureKind::UvMissing,
            }),
            2
        );
        assert_eq!(
            exit_code(&RuntimeStatus::Failed {
                message: "offline".into(),
                kind: RuntimeFailureKind::Offline,
            }),
            1
        );
        assert_eq!(exit_code(&RuntimeStatus::Absent), 1);
    }
}
