//! Provisioner behavior against stub uv binaries — no network, no real uv.
//! Unix-gated (see `mod` declaration): the stubs are shell scripts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::provision::{RuntimeFailureKind, RuntimeProvisioner, RuntimeStatus};
use super::tests::{next_event, temp_paths};
use super::{PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};

/// Writes an executable stub standing in for uv and returns its path.
/// Every invocation appends one line to `counter`.
fn write_stub(dir: &Path, counter: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join("uv");
    let script = format!("#!/bin/sh\necho run >> \"{}\"\n{body}\n", counter.display());
    std::fs::write(&path, script).expect("write stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

fn runs(counter: &Path) -> usize {
    std::fs::read_to_string(counter)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_successful_install_reports_detail_and_ends_ready() {
    let dir = tempfile::tempdir().expect("temp dir");
    let tools = dir.path().join("tools");
    let counter = dir.path().join("runs");
    let stub = write_stub(
        dir.path(),
        &counter,
        r#"echo "Downloading cpython-3.14.0" >&2
mkdir -p "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin"
touch "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin/python3"
exit 0"#,
    );

    let provisioner = RuntimeProvisioner::new(tools.clone(), Some(stub));
    assert_eq!(provisioner.status(), RuntimeStatus::Absent);
    let mut events = provisioner.subscribe();

    assert_eq!(provisioner.ensure().await, RuntimeStatus::Ready);
    assert_eq!(provisioner.status(), RuntimeStatus::Ready);
    assert!(super::provision::runtime_present(&tools));

    let mut seen_detail = false;
    while let Ok(event) = events.try_recv() {
        if let RuntimeStatus::Installing { detail: Some(line) } = event {
            assert!(line.contains("Downloading"), "detail carries uv output");
            seen_detail = true;
        }
    }
    assert!(
        seen_detail,
        "stderr lines must surface as Installing detail"
    );
}

#[tokio::test]
async fn a_failed_install_classifies_offline_and_retries() {
    let dir = tempfile::tempdir().expect("temp dir");
    let counter = dir.path().join("runs");
    let stub = write_stub(
        dir.path(),
        &counter,
        r#"echo "error sending request for url (https://example.invalid)" >&2
exit 1"#,
    );

    let provisioner = RuntimeProvisioner::new(dir.path().join("tools"), Some(stub));
    let outcome = provisioner.ensure().await;
    match outcome {
        RuntimeStatus::Failed { message, kind } => {
            assert_eq!(kind, RuntimeFailureKind::Offline);
            assert!(message.contains("error sending request"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }

    // A failure poisons nothing: the next ensure() runs the install again.
    let _ = provisioner.ensure().await;
    assert_eq!(runs(&counter), 2, "each ensure() after a failure retries");
}

#[tokio::test]
async fn concurrent_ensure_calls_share_one_install() {
    let dir = tempfile::tempdir().expect("temp dir");
    let counter = dir.path().join("runs");
    let stub = write_stub(
        dir.path(),
        &counter,
        r#"sleep 0.2
mkdir -p "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin"
touch "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin/python3"
exit 0"#,
    );

    let provisioner = RuntimeProvisioner::new(dir.path().join("tools"), Some(stub));
    let (first, second) = tokio::join!(provisioner.ensure(), provisioner.ensure());
    assert_eq!(first, RuntimeStatus::Ready);
    assert_eq!(second, RuntimeStatus::Ready);
    assert_eq!(runs(&counter), 1, "the second caller waits for the first");
}

#[tokio::test]
async fn a_present_runtime_never_spawns_uv() {
    let dir = tempfile::tempdir().expect("temp dir");
    let tools = dir.path().join("tools");
    std::fs::create_dir_all(tools.join("cpython-3.14.2-linux/bin")).expect("mkdir");
    std::fs::write(tools.join("cpython-3.14.2-linux/bin/python3"), b"x").expect("write");
    let counter = dir.path().join("runs");
    let stub = write_stub(dir.path(), &counter, "exit 1");

    let provisioner = RuntimeProvisioner::new(tools, Some(stub));
    assert_eq!(provisioner.status(), RuntimeStatus::Ready);
    assert_eq!(provisioner.ensure().await, RuntimeStatus::Ready);
    assert_eq!(runs(&counter), 0, "Ready must be a pure fast path");
}

#[tokio::test]
async fn concurrent_ensure_calls_share_one_failed_attempt() {
    let dir = tempfile::tempdir().expect("temp dir");
    let counter = dir.path().join("runs");
    let stub = write_stub(
        dir.path(),
        &counter,
        r#"sleep 0.2
echo "error sending request for url (https://example.invalid)" >&2
exit 1"#,
    );

    let provisioner = RuntimeProvisioner::new(dir.path().join("tools"), Some(stub));
    let outcomes = tokio::join!(
        provisioner.ensure(),
        provisioner.ensure(),
        provisioner.ensure()
    );
    for outcome in [outcomes.0, outcomes.1, outcomes.2] {
        assert!(
            matches!(
                outcome,
                RuntimeStatus::Failed {
                    kind: RuntimeFailureKind::Offline,
                    ..
                }
            ),
            "{outcome:?}"
        );
    }
    assert_eq!(
        runs(&counter),
        1,
        "queued callers share the failed attempt instead of repeating it"
    );

    let _ = provisioner.ensure().await;
    assert_eq!(runs(&counter), 2, "a later call is a new attempt");
}

const WAITER_MANIFEST: &str = r#"{
  "id": "waiter",
  "name": "Waiter",
  "version": "0.1.0",
  "protocolVersion": 1,
  "runtime": "python",
  "entry": "plugin.py",
  "tiles": [{"id": "w", "name": "W"}]
}"#;

/// One python plugin whose stub uv fails `python install` until `release`
/// exists and rejects every other command, so the plugin never really runs.
async fn waiting_plugin(
    dir: &Path,
    paths: &SmabarPaths,
    counter: &Path,
    release: &Path,
) -> PluginSupervisor {
    let stub = write_stub(
        dir,
        counter,
        &format!(
            r#"if [ "$1" = python ] && [ "$2" = install ]; then
  if [ -e "{release}" ]; then
    mkdir -p "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin"
    touch "$UV_PYTHON_INSTALL_DIR/cpython-3.14.0-stub/bin/python3"
    exit 0
  fi
  echo "error sending request for url (https://example.invalid)" >&2
  exit 1
fi
echo "stub uv rejects $1" >&2
exit 1"#,
            release = release.display()
        ),
    );
    let plugin_dir = paths.plugins_dir().join("waiter");
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    std::fs::write(plugin_dir.join("smabar.json"), WAITER_MANIFEST).expect("manifest");
    std::fs::write(
        plugin_dir.join("plugin.py"),
        "# /// script\n# dependencies = []\n# ///\n",
    )
    .expect("script");
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("config watcher"));
    PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        config,
        SupervisorOptions {
            uv_override: Some(stub),
            python_install_dir: Some(paths.tools_dir()),
            ..SupervisorOptions::default()
        },
    )
    .await
}

/// The plugin's next status event as `(status, error)`.
async fn next_status(
    events: &mut tokio::sync::broadcast::Receiver<PluginEvent>,
) -> (PluginStatus, Option<String>) {
    loop {
        if let PluginEvent::Status { status, error, .. } = next_event(events).await {
            return (status, error);
        }
    }
}

#[tokio::test]
async fn a_python_plugin_waits_for_the_runtime_instead_of_failing() {
    let (dir, paths) = temp_paths();
    let counter = dir.path().join("runs");
    let release = dir.path().join("release");
    let supervisor = waiting_plugin(dir.path(), &paths, &counter, &release).await;
    let mut events = supervisor.subscribe_events();

    assert_eq!(
        next_status(&mut events).await,
        (PluginStatus::Starting, None)
    );
    let (status, reason) = next_status(&mut events).await;
    assert_eq!(status, PluginStatus::Starting);
    assert_eq!(
        reason.as_deref(),
        Some("waiting for the Python runtime: downloading")
    );
    let (status, reason) = next_status(&mut events).await;
    assert_eq!(status, PluginStatus::Starting);
    let reason = reason.expect("the failed install is the wait reason");
    assert!(
        reason.starts_with("waiting for the Python runtime: "),
        "{reason}"
    );
    assert!(reason.contains("error sending request"), "{reason}");
    // The plugin now sleeps out the retry delay: nothing else happens, and
    // in particular no `failed` is reported.
    assert!(
        timeout(Duration::from_secs(2), events.recv())
            .await
            .is_err(),
        "a runtime wait emits nothing until the next attempt"
    );
    assert_eq!(runs(&counter), 1);

    // The network is back and the user presses Retry.
    std::fs::write(&release, b"").expect("release");
    assert_eq!(supervisor.runtime().ensure().await, RuntimeStatus::Ready);
    // Long before its 15 s timer the plugin wakes and attempts a real start;
    // the stub's refusal to sync is a plugin failure, not a runtime wait.
    let outcome = timeout(Duration::from_secs(5), async {
        loop {
            let (status, error) = next_status(&mut events).await;
            if status == PluginStatus::Failed {
                return error.unwrap_or_default();
            }
        }
    })
    .await
    .expect("the plugin left the runtime wait");
    assert!(!outcome.contains("Python runtime"), "{outcome}");
    assert!(outcome.contains("restarting in"), "{outcome}");
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_shutdown_ends_the_runtime_wait_at_once() {
    let (dir, paths) = temp_paths();
    let counter = dir.path().join("runs");
    let release = dir.path().join("never");
    let supervisor = waiting_plugin(dir.path(), &paths, &counter, &release).await;
    let mut events = supervisor.subscribe_events();
    loop {
        let (_, reason) = next_status(&mut events).await;
        if reason.is_some_and(|reason| reason.contains("error sending request")) {
            break;
        }
    }

    timeout(Duration::from_secs(5), supervisor.shutdown_all())
        .await
        .expect("shutdown must not wait for the retry timer");
    let (status, _) = next_status(&mut events).await;
    assert_eq!(status, PluginStatus::Stopped);
    assert_eq!(runs(&counter), 1, "shutdown starts no further attempt");
}
