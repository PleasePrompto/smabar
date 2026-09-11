//! The folder swap behind store installs and updates: one stop, one start,
//! the previous version in the backup, a deactivated plugin left alone.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::tests::temp_paths;
use super::{
    PluginEvent, PluginStatus, PluginSupervisor, ReplaceError, SupervisorOptions, set_plugin_active,
};

const MANIFEST: &str = r#"{
  "id": "swap",
  "name": "Swap",
  "version": "VERSION",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "tiles": [{"id": "main", "name": "Main", "hasFlyout": false}]
}"#;

const SCRIPT: &str = r#"
import json
import pathlib
import sys

for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        data = pathlib.Path(message["params"]["dataDir"])
        data.mkdir(parents=True, exist_ok=True)
        (data / "started-by").write_text("VERSION")
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
    elif message.get("method") == "shutdown":
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
        sys.exit(0)
"#;

fn write_plugin(dir: &Path, version: &str) {
    fs::create_dir_all(dir).expect("create dir");
    fs::write(
        dir.join("smabar.json"),
        MANIFEST.replace("VERSION", version),
    )
    .expect("manifest");
    fs::write(dir.join("main.py"), SCRIPT.replace("VERSION", version)).expect("script");
}

async fn start(paths: &SmabarPaths) -> (Arc<ConfigWatcher>, PluginSupervisor) {
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("config"));
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        Arc::clone(&config),
        SupervisorOptions::default(),
    )
    .await;
    (config, supervisor)
}

async fn wait_for(supervisor: &PluginSupervisor, expected: PluginStatus) {
    let mut events = supervisor.subscribe_events();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if supervisor
            .plugin_infos()
            .iter()
            .any(|info| info.id == "swap" && info.status == expected)
        {
            return;
        }
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("timed out waiting for the plugin")
            .expect("event bus");
        if let PluginEvent::Status {
            plugin_id, status, ..
        } = event
            && plugin_id == "swap"
            && status == expected
        {
            return;
        }
    }
}

#[tokio::test]
async fn a_running_plugin_is_swapped_stopped_once_and_started_once() {
    let (_dir, paths) = temp_paths();
    write_plugin(&paths.plugins_dir().join("swap"), "1");
    let (_config, supervisor) = start(&paths).await;
    wait_for(&supervisor, PluginStatus::Running).await;
    let staged = paths.store_staging_dir().join("swap");
    write_plugin(&staged, "2");
    let backup = paths.store_backup_dir("swap");
    let watcher = supervisor.watch_status("swap");

    let outcome = supervisor
        .replace_dir("swap", &staged, &backup)
        .await
        .expect("replace");

    assert!(outcome.was_running);
    assert!(outcome.backed_up);
    assert!(!staged.exists());
    assert_eq!(
        fs::read_to_string(backup.join("smabar.json")).expect("backup manifest"),
        MANIFEST.replace("VERSION", "1")
    );
    assert_eq!(
        fs::read_to_string(paths.plugins_dir().join("swap").join("smabar.json")).expect("manifest"),
        MANIFEST.replace("VERSION", "2")
    );
    let settled = watcher
        .settle(Duration::from_secs(30), PluginStatus::Starting)
        .await;
    assert_eq!(settled.status, PluginStatus::Running, "{settled:?}");
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("swap").join("started-by")).expect("marker"),
        "2"
    );
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_deactivated_plugin_stays_deactivated_after_the_swap() {
    let (_dir, paths) = temp_paths();
    write_plugin(&paths.plugins_dir().join("swap"), "1");
    let (config, supervisor) = start(&paths).await;
    wait_for(&supervisor, PluginStatus::Running).await;
    set_plugin_active(&config, "swap", false).expect("deactivate");
    wait_for(&supervisor, PluginStatus::Deactivated).await;
    let staged = paths.store_staging_dir().join("swap");
    write_plugin(&staged, "2");

    let outcome = supervisor
        .replace_dir("swap", &staged, &paths.store_backup_dir("swap"))
        .await
        .expect("replace");

    assert!(!outcome.was_running);
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(
        supervisor
            .plugin_infos()
            .iter()
            .any(|info| { info.id == "swap" && info.status == PluginStatus::Deactivated })
    );
    // Version 1 ran before the deactivation; version 2 must not have.
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("swap").join("started-by")).expect("marker"),
        "1"
    );
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_fresh_folder_is_started_by_the_watcher() {
    let (_dir, paths) = temp_paths();
    let (_config, supervisor) = start(&paths).await;
    let staged = paths.store_staging_dir().join("swap");
    write_plugin(&staged, "1");
    let watcher = supervisor.watch_status("swap");

    let outcome = supervisor
        .replace_dir("swap", &staged, &paths.store_backup_dir("swap"))
        .await
        .expect("replace");

    assert!(!outcome.backed_up);
    let settled = watcher
        .settle(Duration::from_secs(30), PluginStatus::Starting)
        .await;
    assert_eq!(settled.status, PluginStatus::Running, "{settled:?}");
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_missing_staged_folder_or_bad_id_is_refused() {
    let (_dir, paths) = temp_paths();
    let (_config, supervisor) = start(&paths).await;
    let error = supervisor
        .replace_dir(
            "swap",
            &paths.store_staging_dir().join("nope"),
            &paths.store_backup_dir("swap"),
        )
        .await
        .expect_err("missing");
    assert!(matches!(error, ReplaceError::StagedMissing { .. }));
    let error = supervisor
        .replace_dir(
            "Bad Id",
            &paths.store_staging_dir(),
            &paths.store_backup_dir("x"),
        )
        .await
        .expect_err("bad id");
    assert!(matches!(error, ReplaceError::InvalidId { .. }));
    supervisor.shutdown_all().await;
}
