//! Provisioner behavior against stub uv binaries — no network, no real uv.
//! Unix-gated (see `mod` declaration): the stubs are shell scripts.

use std::path::{Path, PathBuf};

use super::provision::{RuntimeFailureKind, RuntimeProvisioner, RuntimeStatus};

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
