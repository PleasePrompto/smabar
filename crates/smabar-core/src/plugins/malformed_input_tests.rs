//! End-to-end regressions for tolerant plugin-author input handling.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::config::ConfigWatcher;
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};

const MANIFEST: &str = r#"{
  "id": "tolerant",
  "name": "Tolerant Plugin",
  "version": "1.0.0",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "futureRoot": true,
  "tiles": [{"id": "w", "name": "W", "futureTile": true}]
}"#;

const MISSING_ENTRY_MANIFEST: &str = r#"{
  "id": "tolerant",
  "name": "Tolerant Plugin",
  "version": "1.0.1",
  "protocolVersion": 1,
  "runtime": "python",
  "entry": "missing.py",
  "futureRejected": true,
  "tiles": [{"id": "w", "name": "W"}]
}"#;

const SCRIPT: &str = r#"
import json
import sys

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.stdout.write("x" * (1024 * 1024 + 1) + "\n")
        sys.stdout.flush()
        for _ in range(20):
            send({"jsonrpc": "2.0", "method": "ui.render", "params": {
                "tileId": "w", "target": "popup", "html": "still rendered",
                "ttlMs": "forever"}})
        for _ in range(2):
            send({"jsonrpc": "2.0", "method": "future.notification", "params": {}})
    elif method == "event":
        send({"jsonrpc": "2.0", "method": "ui.render", "params": {
            "tileId": "w", "target": "flyout", "html": "still running"}})
    elif method in ("ping", "shutdown"):
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        if method == "shutdown":
            sys.exit(0)
"#;

async fn wait_for_log(path: &Path, needle: &str) -> String {
    timeout(Duration::from_secs(10), async {
        loop {
            let log = fs::read_to_string(path).unwrap_or_default();
            if log.contains(needle) {
                return log;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {needle:?} in {}", path.display()))
}

#[tokio::test]
async fn malformed_optional_input_is_ignored_logged_once_and_keeps_running() {
    let (_temp, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("tolerant");
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
    assert!(supervisor.hot_reload_enabled(), "test requires the watcher");
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
    loop {
        if let PluginEvent::UiRender { ttl_ms, html, .. } = next_event(&mut events).await {
            assert_eq!(html, "still rendered");
            assert_eq!(
                ttl_ms,
                Some(5_000),
                "invalid ttlMs must not create a sticky popup"
            );
            break;
        }
    }

    let log_path = paths.logs_dir().join("plugin-tolerant.log");
    wait_for_log(&log_path, "exceeded 1048576 bytes").await;
    wait_for_log(&log_path, "future.notification").await;

    fs::write(plugin_dir.join("smabar.json"), MISSING_ENTRY_MANIFEST)
        .expect("point manifest at a missing entry");
    wait_for_log(&log_path, "missing.py").await;
    fs::write(plugin_dir.join("smabar.json"), "{ broken").expect("break manifest");
    wait_for_log(&log_path, "not a valid manifest").await;
    supervisor
        .dispatch_action("tolerant", "w", "open", None)
        .await
        .expect("the known-good process is still running");
    loop {
        if let PluginEvent::UiRender { target, html, .. } = next_event(&mut events).await
            && target == "flyout"
        {
            assert_eq!(html, "still running");
            break;
        }
    }
    supervisor.shutdown_all().await;

    let entries: Vec<serde_json::Value> = fs::read_to_string(&log_path)
        .expect("plugin log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL entry"))
        .filter(|entry: &serde_json::Value| entry["source"] == "core")
        .collect();
    for needle in [
        "futureRoot",
        "exceeded 1048576 bytes",
        "ttlMs",
        "future.notification",
        "missing.py",
        "not a valid manifest",
    ] {
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["message"]
                    .as_str()
                    .is_some_and(|text| text.contains(needle)))
                .count(),
            1,
            "{needle:?} was not logged exactly once: {entries:?}"
        );
    }
    assert!(
        entries.iter().all(|entry| !entry["message"]
            .as_str()
            .is_some_and(|text| text.contains("futureRejected"))),
        "a rejected replacement must not claim its optional parts were kept usable: {entries:?}"
    );
}
