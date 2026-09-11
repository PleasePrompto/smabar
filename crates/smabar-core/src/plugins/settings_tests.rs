//! Regression tests for settings delivery: pushes during the crash-backoff
//! window and the initialize window must still reach the plugin.

use std::fs;
use std::sync::Arc;

use serde_json::json;

use crate::config::ConfigWatcher;
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};

/// Minimal exec-runtime manifest running `python3 main.py` with one tile.
fn exec_manifest(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","name":"{id}","version":"0.1.0","protocolVersion":1,
            "runtime":"exec","command":["python3","main.py"],
            "tiles":[{{"id":"w","name":"W"}}]}}"#
    )
}

/// Exits with code 1 on its first run (dotfile marker — invisible to the
/// folder watcher); afterwards it answers initialize and renders the
/// settings it received.
const CRASH_ONCE_SCRIPT: &str = r#"
import json
import os
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if not os.path.exists(".crashed-once"):
    open(".crashed-once", "w").close()
    sys.exit(1)

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w", "target": "tile",
                         "html": "init:" + json.dumps(msg["params"]["settings"])}})
    elif method == "ping":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.exit(0)
"#;

/// Renders its initialize settings, then stalls the initialize RESPONSE for
/// 1.5 s — a deterministic window between the core's settings snapshot and
/// the Running command loop. settings.changed pushes render as "flyout".
const SLOW_INIT_SCRIPT: &str = r#"
import json
import sys
import time


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w", "target": "tile",
                         "html": "init:" + json.dumps(msg["params"]["settings"])}})
        time.sleep(1.5)
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "settings.changed":
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w", "target": "flyout",
                         "html": "changed:" + json.dumps(msg["params"]["settings"])}})
    elif method == "ping":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.exit(0)
"#;

#[tokio::test]
async fn settings_changed_during_backoff_arrive_via_initialize() {
    // Regression: a settings push during the crash-backoff window is
    // discarded by backoff_wait — the next run's initialize must read the
    // settings fresh so the plugin still starts with the new state.
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("crashonce");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), exec_manifest("crashonce")).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), CRASH_ONCE_SCRIPT).expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        Arc::clone(&config),
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();

    // The first run crashes before answering initialize.
    loop {
        if let PluginEvent::Status {
            status: PluginStatus::Failed,
            ..
        } = next_event(&mut events).await
        {
            break;
        }
    }
    // The lifecycle now sits in its backoff sleep; change the settings.
    let mut new_config = config.current();
    new_config
        .plugins
        .insert("crashonce".to_string(), json!({"city": "Berlin"}));
    config.apply(new_config).expect("apply during backoff");

    // The restarted run must see the new settings in its initialize params.
    let html = loop {
        if let PluginEvent::UiRender { target, html, .. } = next_event(&mut events).await {
            assert_eq!(target, "tile");
            break html;
        }
    };
    assert!(html.contains("Berlin"), "initialize settings stale: {html}");
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn settings_changed_during_initialize_window_are_pushed_after_running() {
    // Regression: a change landing AFTER the core read the settings for
    // initialize but BEFORE the command loop serves must still reach the
    // plugin — as a settings.changed push right after Running.
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("slowinit");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), exec_manifest("slowinit")).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), SLOW_INIT_SCRIPT).expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        Arc::clone(&config),
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();

    // The tile render proves the initialize snapshot was already taken; the
    // plugin now stalls its initialize response for 1.5 s.
    let html = loop {
        if let PluginEvent::UiRender { target, html, .. } = next_event(&mut events).await {
            assert_eq!(target, "tile");
            break html;
        }
    };
    assert!(!html.contains("Berlin"), "snapshot must predate the change");

    let mut new_config = config.current();
    new_config
        .plugins
        .insert("slowinit".to_string(), json!({"city": "Berlin"}));
    config.apply(new_config).expect("apply during initialize");

    // After Running, the plugin must receive the new settings via
    // settings.changed (rendered as "flyout").
    let html = loop {
        if let PluginEvent::UiRender { target, html, .. } = next_event(&mut events).await
            && target == "flyout"
        {
            break html;
        }
    };
    assert!(html.contains("Berlin"), "settings.changed missing: {html}");
    supervisor.shutdown_all().await;
}
