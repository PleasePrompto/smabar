use std::cell::Cell;

use smabar_core::config::{ConfigWatcher, SmabarConfig, SmabarPaths};

use super::{apply_registration, initialize};

#[tokio::test]
async fn default_is_once_only_and_external_opt_out_survives_restarts() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    // An old config has no marker and must retain unrelated preferences.
    std::fs::create_dir_all(paths.base_dir()).expect("create config directory");
    std::fs::write(paths.config_file(), r#"{"language":"de"}"#).expect("old config");
    let watcher = ConfigWatcher::spawn(paths.clone()).expect("watch config");
    let calls = Cell::new(0);
    let enable = || {
        assert!(
            SmabarConfig::load(&paths)
                .expect("saved marker")
                .autostart_initialized
        );
        calls.set(calls.get() + 1);
        Ok(true)
    };
    initialize(&watcher, false, enable).expect("development startup");
    assert_eq!(calls.get(), 0);
    assert!(!watcher.current().autostart_initialized);
    initialize(&watcher, true, enable).expect("first release startup");
    assert_eq!(calls.get(), 1);
    assert_eq!(watcher.current().language, "de");
    // Reopening the saved config simulates a restart after external removal.
    drop(watcher);
    let watcher = ConfigWatcher::spawn(paths).expect("reopen config");
    initialize(&watcher, true, || {
        panic!("must not restore removed registration")
    })
    .expect("later startup");
}

#[tokio::test]
async fn registration_failure_is_reported_and_never_retried_automatically() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    let watcher = ConfigWatcher::spawn(paths).expect("watch config");
    let result = initialize(&watcher, true, || anyhow::bail!("registration denied"));
    assert!(
        result
            .expect_err("registration must fail")
            .to_string()
            .contains("denied")
    );
    assert!(watcher.current().autostart_initialized);
    initialize(&watcher, true, || panic!("failed default must not retry"))
        .expect("subsequent startup");
}

#[tokio::test]
async fn failed_marker_write_prevents_any_os_registration() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    let watcher = ConfigWatcher::spawn(paths.clone()).expect("watch config");
    // Deterministic even as root: the atomic writer cannot replace a directory.
    std::fs::create_dir(paths.config_file().with_extension("json.tmp"))
        .expect("block temporary file");
    initialize(&watcher, true, || panic!("must not register before saving"))
        .expect_err("marker write must fail");
    assert!(!watcher.current().autostart_initialized);
}

#[test]
fn settings_and_tray_changes_are_idempotent_and_read_back_the_os() {
    let registered = Cell::new(false);
    let writes = Cell::new(0);
    let apply = |enabled| {
        apply_registration(
            enabled,
            || Ok(registered.get()),
            |next| {
                writes.set(writes.get() + 1);
                registered.set(next);
                Ok(())
            },
        )
    };
    assert!(apply(true).expect("enable"));
    assert!(apply(true).expect("enable again"));
    assert!(!apply(false).expect("disable"));
    assert!(!apply(false).expect("disable again"));
    assert_eq!(writes.get(), 2);
    // Tray toggling uses the freshly observed OS value, not an old menu check.
    registered.set(true);
    assert!(!apply(!registered.get()).expect("toggle external state"));
}

#[test]
fn read_write_and_unconfirmed_change_errors_cannot_be_reported_as_success() {
    apply_registration(
        true,
        || anyhow::bail!("read denied"),
        |_| panic!("no write"),
    )
    .expect_err("read failure");
    apply_registration(true, || Ok(false), |_| anyhow::bail!("write denied"))
        .expect_err("write failure");
    apply_registration(true, || Ok(false), |_| Ok(())).expect_err("OS did not confirm the change");
    let reads = Cell::new(0);
    apply_registration(
        true,
        || {
            reads.set(reads.get() + 1);
            if reads.get() == 1 {
                Ok(false)
            } else {
                anyhow::bail!("read-back failed")
            }
        },
        |_| Ok(()),
    )
    .expect_err("read-back failure");
}
