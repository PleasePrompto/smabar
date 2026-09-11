use super::*;

#[test]
fn probe_rejects_missing_partial_and_wrong_versions() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(!runtime_present(dir.path()), "empty dir has no runtime");
    // Interrupted download: version dir exists, interpreter does not.
    std::fs::create_dir_all(dir.path().join("cpython-3.14.2-linux/bin")).expect("mkdir");
    assert!(!runtime_present(dir.path()), "no interpreter file yet");
    // A different interpreter line does not count.
    std::fs::create_dir_all(dir.path().join("cpython-3.12.9-linux/bin")).expect("mkdir");
    std::fs::write(dir.path().join("cpython-3.12.9-linux/bin/python3"), b"x").expect("write");
    assert!(!runtime_present(dir.path()), "3.12 is not the managed line");
}

#[test]
fn probe_accepts_unix_and_windows_layouts() {
    let unix = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(unix.path().join("cpython-3.14.2-linux/bin")).expect("mkdir");
    std::fs::write(unix.path().join("cpython-3.14.2-linux/bin/python3"), b"x").expect("write");
    assert!(runtime_present(unix.path()));

    let windows = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(windows.path().join("cpython-3.14.2-windows")).expect("mkdir");
    std::fs::write(
        windows.path().join("cpython-3.14.2-windows/python.exe"),
        b"x",
    )
    .expect("write");
    assert!(runtime_present(windows.path()));
}

#[test]
fn failure_classification_spots_network_errors() {
    for text in [
        "error: Failed to fetch: https://…",
        "Caused by: error sending request for url",
        "connect: Connection refused",
        "request or response body error: operation timed out",
        "dns error: failed to lookup address",
    ] {
        assert_eq!(
            classify_failure(text),
            RuntimeFailureKind::Offline,
            "{text}"
        );
    }
    assert_eq!(
        classify_failure("error: no download found for request"),
        RuntimeFailureKind::Other
    );
}

#[test]
fn only_parked_python_plugins_are_revivable() {
    let entries = vec![
        ("parked".into(), PluginRuntime::Python, true),
        ("running".into(), PluginRuntime::Python, false),
        ("exec".into(), PluginRuntime::Exec, true),
    ];
    let revived = revivable(entries.into_iter());
    assert_eq!(revived, vec!["parked".to_string()]);
}
