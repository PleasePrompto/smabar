//! The code folder and the writable data directory must stay separate.
//!
//! Split out of `tests.rs` for the 500-line limit. This guards the invariant
//! that plugin writes cannot touch the watched code folder and restart the
//! process in a loop.

use std::fs;
use std::sync::Arc;

use crate::config::ConfigWatcher;
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{PluginEvent, PluginSupervisor, SupervisorOptions};

const MANIFEST: &str = r#"{
  "id": "dirplug",
  "name": "Dir Plugin",
  "version": "0.1.0",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "tiles": [{"id": "w1", "name": "Tile One"}]
}"#;

/// Echoes both directories back so the test can assert on the real handshake
/// instead of on the payload builder.
const SCRIPT: &str = r#"
import json
import os
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        params = msg["params"]
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w1", "target": "tile",
                         "html": json.dumps({
                             "dataDir": params.get("dataDir"),
                             "pluginDir": params.get("pluginDir"),
                             "envDataDir": os.environ.get("SMABAR_PLUGIN_DATA_DIR"),
                             "envPluginDir": os.environ.get("SMABAR_PLUGIN_DIR"),
                             "dataDirExists": os.path.isdir(params.get("dataDir", "")),
                         })}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.exit(0)
"#;

#[tokio::test]
async fn the_handshake_hands_over_a_writable_dir_outside_the_watched_code_folder() {
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("dirplug");
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

    let html = loop {
        match next_event(&mut events).await {
            PluginEvent::UiRender { html, .. } => break html,
            _ => continue,
        }
    };
    let reported: serde_json::Value = serde_json::from_str(&html).expect("plugin echoed json");

    let expected_data = paths.plugin_data_dir("dirplug");
    assert_eq!(reported["dataDir"], expected_data.display().to_string());
    assert_eq!(reported["pluginDir"], plugin_dir.display().to_string());
    assert_eq!(reported["envDataDir"], reported["dataDir"]);
    assert_eq!(reported["envPluginDir"], reported["pluginDir"]);
    // Ready before the first handler runs — a plugin may write immediately.
    assert_eq!(reported["dataDirExists"], serde_json::Value::Bool(true));
    // The whole point: writes there cannot reach the folder watcher.
    assert!(!expected_data.starts_with(paths.plugins_dir()));

    supervisor.shutdown_all().await;
}
