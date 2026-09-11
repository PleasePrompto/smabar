//! Concurrent reload regression tests at the public supervisor boundary.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::config::ConfigWatcher;
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{PluginError, PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};

const MANIFEST: &str = r#"{
  "id": "serial",
  "name": "Serialized Lifecycle",
  "version": "0.1.0",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "tiles": [{"id": "w", "name": "W"}]
}"#;

/// Records every live process in the unwatched data directory. Shutdown
/// pauses on a test-owned release file, making the stop/start race observable
/// without sleeps inside the supervisor.
const SCRIPT: &str = r#"
import glob
import json
import os
import sys
import time

marker = None
data_dir = None


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        data_dir = msg["params"]["dataDir"]
        os.makedirs(data_dir, exist_ok=True)
        live = glob.glob(os.path.join(data_dir, "instance-*"))
        if live:
            with open(os.path.join(data_dir, "overlap"), "w") as handle:
                handle.write("\n".join(live))
        marker = os.path.join(data_dir, "instance-" + str(os.getpid()))
        open(marker, "w").close()
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "ping":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "shutdown":
        open(os.path.join(data_dir, "shutdown-received"), "w").close()
        release = os.path.join(data_dir, "release-shutdown")
        while not os.path.exists(release):
            time.sleep(0.01)
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        if marker is not None and os.path.exists(marker):
            os.remove(marker)
        sys.exit(0)
"#;

async fn wait_for_file(path: &Path) {
    timeout(Duration::from_secs(10), async {
        while !path.is_file() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {}", path.display()));
}

#[tokio::test]
async fn watcher_reload_and_explicit_restart_never_overlap_plugin_processes() {
    let (_temp, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("serial");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), MANIFEST).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), SCRIPT).expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    assert!(
        supervisor.hot_reload_enabled(),
        "test requires the real watcher"
    );
    let mut events = supervisor.subscribe_events();
    loop {
        if let PluginEvent::Status {
            status: PluginStatus::Running,
            ..
        } = next_event(&mut events).await
        {
            break;
        }
    }

    let data_dir = paths.plugin_data_dir("serial");
    let shutdown_received = data_dir.join("shutdown-received");
    fs::write(
        plugin_dir.join("main.py"),
        format!("{SCRIPT}\n# watcher edit\n"),
    )
    .expect("trigger watcher reload");
    wait_for_file(&shutdown_received).await;

    let mut concurrent = {
        let supervisor = supervisor.clone();
        tokio::spawn(async move { supervisor.restart("serial").await })
    };

    // While the watcher-owned stop is paused, a correct explicit restart is
    // queued behind it. The old implementation started a second process in
    // this gap, which records `overlap` during initialize.
    let finished_before_release = match timeout(Duration::from_secs(1), &mut concurrent).await {
        Ok(result) => {
            result
                .expect("restart task panicked")
                .expect("restart failed");
            wait_for_file(&data_dir.join("overlap")).await;
            true
        }
        Err(_) => false,
    };
    fs::write(data_dir.join("release-shutdown"), "release").expect("release plugin shutdown");
    if !finished_before_release {
        timeout(Duration::from_secs(10), concurrent)
            .await
            .expect("concurrent restart timed out")
            .expect("restart task panicked")
            .expect("restart failed");
    }

    assert!(
        !data_dir.join("overlap").exists(),
        "watcher reload and explicit restart ran two instances of one plugin"
    );
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn config_overflow_converges_on_the_latest_activation_state() {
    let (_temp, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("serial");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), MANIFEST).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), SCRIPT).expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        Arc::clone(&config),
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();
    loop {
        if let PluginEvent::Status {
            status: PluginStatus::Running,
            ..
        } = next_event(&mut events).await
        {
            break;
        }
    }

    let mut snapshot = config.current();
    snapshot.plugins_deactivated = vec!["serial".to_owned()];
    config.apply(snapshot).expect("deactivate plugin");
    let data_dir = paths.plugin_data_dir("serial");
    wait_for_file(&data_dir.join("shutdown-received")).await;

    // Queue the reactivation first, then overflow the 16-entry broadcast
    // while shutdown is blocked. Losing that first entry used to leave the
    // process off even though the authoritative config said it was active.
    let mut snapshot = config.current();
    snapshot.plugins_deactivated.clear();
    config.apply(snapshot).expect("reactivate plugin");
    for sequence in 0..20 {
        let mut snapshot = config.current();
        snapshot.plugins.insert(
            "unrelated".to_owned(),
            serde_json::json!({ "sequence": sequence }),
        );
        config.apply(snapshot).expect("fill config broadcast");
    }
    fs::write(data_dir.join("release-shutdown"), "release").expect("release plugin shutdown");

    timeout(Duration::from_secs(10), async {
        loop {
            if let PluginEvent::Status {
                plugin_id,
                status: PluginStatus::Running,
                ..
            } = next_event(&mut events).await
                && plugin_id == "serial"
            {
                break;
            }
        }
    })
    .await
    .expect("latest active config did not restart the plugin");

    assert!(config.current().plugins_deactivated.is_empty());
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn shutdown_waits_for_watcher_transitions_and_is_terminal() {
    let (_temp, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("serial");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), MANIFEST).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), SCRIPT).expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();
    loop {
        if let PluginEvent::Status {
            status: PluginStatus::Running,
            ..
        } = next_event(&mut events).await
        {
            break;
        }
    }

    fs::write(
        plugin_dir.join("main.py"),
        format!("{SCRIPT}\n# watcher edit\n"),
    )
    .expect("trigger watcher reload");
    let data_dir = paths.plugin_data_dir("serial");
    wait_for_file(&data_dir.join("shutdown-received")).await;

    let mut shutdown = {
        let supervisor = supervisor.clone();
        tokio::spawn(async move { supervisor.shutdown_all().await })
    };
    let returned_while_watcher_still_owned_the_process =
        timeout(Duration::from_millis(100), &mut shutdown)
            .await
            .is_ok();
    fs::write(data_dir.join("release-shutdown"), "release").expect("release plugin shutdown");
    if !returned_while_watcher_still_owned_the_process {
        timeout(Duration::from_secs(10), shutdown)
            .await
            .expect("shutdown timed out")
            .expect("shutdown task panicked");
    }

    assert!(!returned_while_watcher_still_owned_the_process);
    assert!(supervisor.current_plugins().is_empty());
    assert!(matches!(
        supervisor.restart("serial").await,
        Err(PluginError::NotRunning { .. })
    ));
    supervisor.shutdown_all().await;
}
