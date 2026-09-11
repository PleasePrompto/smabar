//! Integration tests with a real exec-runtime plugin process (python3).
//!
//! These run on the tokio current-thread test runtime: spawned tasks only
//! run once the test awaits, so subscribing to events right after
//! `PluginSupervisor::start` deterministically sees every event.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::{PluginError, PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};

const MANIFEST: &str = r#"{
  "id": "testplug",
  "name": "Test Plugin",
  "version": "0.1.0",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "tiles": [
    {"id": "w1", "name": "Tile One", "hasFlyout": true}
  ]
}"#;

const SCRIPT: &str = r#"
import json
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        print("hello from stdout")
        send({"jsonrpc": "2.0", "method": "log",
              "params": {"level": "info", "message": "plugin started",
                         "fields": {"language": msg["params"]["language"]}}})
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w1", "target": "tile", "html": "<b>hi</b>"}})
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w1", "target": "popup", "html": "Heads up",
                         "ttlMs": 9000}})
    elif method == "event":
        action = msg["params"]["action"]
        send({"jsonrpc": "2.0", "method": "ui.render",
              "params": {"tileId": "w1", "target": "flyout",
                         "html": "clicked:" + action}})
    elif method == "ping":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.exit(0)
"#;

pub(super) fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    (dir, paths)
}

pub(super) async fn next_event(rx: &mut broadcast::Receiver<PluginEvent>) -> PluginEvent {
    timeout(Duration::from_secs(30), rx.recv())
        .await
        .expect("timed out waiting for a plugin event")
        .expect("event channel closed")
}

fn assert_status(event: PluginEvent, plugin_id: &str, expected: PluginStatus) {
    match event {
        PluginEvent::Status {
            plugin_id: id,
            status,
            ..
        } => {
            assert_eq!(id, plugin_id);
            assert_eq!(status, expected);
        }
        other => panic!("expected status {expected:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn exec_plugin_initializes_renders_dispatches_and_shuts_down() {
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("testplug");
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

    match next_event(&mut events).await {
        PluginEvent::Added {
            plugin_id,
            name,
            tiles,
            ..
        } => {
            assert_eq!(plugin_id, "testplug");
            assert_eq!(name, "Test Plugin");
            assert_eq!(tiles.len(), 1);
            assert!(tiles[0].has_flyout);
        }
        other => panic!("expected Added, got {other:?}"),
    }
    assert_status(
        next_event(&mut events).await,
        "testplug",
        PluginStatus::Starting,
    );
    assert_status(
        next_event(&mut events).await,
        "testplug",
        PluginStatus::Running,
    );
    match next_event(&mut events).await {
        PluginEvent::UiRender {
            plugin_id,
            tile_id,
            target,
            html,
            ttl_ms,
        } => {
            assert_eq!(plugin_id, "testplug");
            assert_eq!(tile_id, "w1");
            assert_eq!(target, "tile");
            assert_eq!(html, "<b>hi</b>");
            assert_eq!(ttl_ms, None);
        }
        other => panic!("expected UiRender, got {other:?}"),
    }

    match next_event(&mut events).await {
        PluginEvent::UiRender {
            target,
            html,
            ttl_ms,
            ..
        } => {
            assert_eq!(target, "popup");
            assert_eq!(html, "Heads up");
            assert_eq!(ttl_ms, Some(9_000));
        }
        other => panic!("expected popup UiRender, got {other:?}"),
    }

    match supervisor
        .dispatch_action("testplug", "missing", "open", None)
        .await
        .expect_err("manifest-local tile ids are enforced")
    {
        PluginError::UnknownTile {
            tile_id, available, ..
        } => {
            assert_eq!(tile_id, "missing");
            assert_eq!(available, ["w1"]);
        }
        other => panic!("expected UnknownTile, got {other:?}"),
    }

    supervisor
        .dispatch_action("testplug", "w1", "open", None)
        .await
        .expect("dispatch action");
    match next_event(&mut events).await {
        PluginEvent::UiRender { target, html, .. } => {
            assert_eq!(target, "flyout");
            assert_eq!(html, "clicked:open");
        }
        other => panic!("expected UiRender for the action, got {other:?}"),
    }

    // Late subscribers (a shell that attaches or reloads after these pushes)
    // replay the cached renders instead of waiting for the next push. The
    // tracking task runs concurrently — poll briefly until it caught up.
    let mut ui = supervisor.current_ui();
    for _ in 0..50 {
        if ui.len() == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        ui = supervisor.current_ui();
    }
    assert_eq!(ui.len(), 2, "tile + flyout renders must be cached");
    assert!(ui.iter().any(|s| s.plugin_id == "testplug"
        && s.tile_id == "w1"
        && s.target == "tile"
        && s.html == "<b>hi</b>"));
    assert!(
        ui.iter()
            .any(|s| s.target == "flyout" && s.html == "clicked:open")
    );
    assert!(ui.iter().all(|snapshot| snapshot.target != "popup"));

    supervisor.shutdown_all().await;
    // shutdown_all only returns after the lifecycle task finished, so the
    // Stopped event is already queued.
    assert_status(
        next_event(&mut events).await,
        "testplug",
        PluginStatus::Stopped,
    );

    // Auto-captured stdout plus the log RPC both land in the plugin log file.
    let log = fs::read_to_string(paths.logs_dir().join("plugin-testplug.log"))
        .expect("plugin log file exists");
    let entries: Vec<serde_json::Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).expect("log line is valid JSON"))
        .collect();
    assert!(
        entries.iter().any(|e| e["source"] == "stdout"
            && e["level"] == "info"
            && e["message"] == "hello from stdout"),
        "missing stdout capture in {entries:?}"
    );
    // The fields echo the initialize params: the config language code reached
    // the plugin (default config → "en").
    assert!(
        entries.iter().any(|e| e["source"] == "log"
            && e["level"] == "info"
            && e["message"] == "plugin started"
            && e["fields"]["language"] == "en"),
        "missing log RPC entry with initialize language in {entries:?}"
    );

    // After shutdown_all the plugin is deregistered.
    assert!(matches!(
        supervisor
            .dispatch_action("testplug", "w1", "x", None)
            .await,
        Err(PluginError::UnknownPlugin { .. })
    ));
}

#[tokio::test]
async fn output_of_a_fast_dying_plugin_reaches_its_log() {
    // Regression: captured stdout/stderr used to be dropped with the incoming
    // channel when a plugin died before answering initialize — losing exactly
    // the output needed to debug a crashing plugin.
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("crashy");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        plugin_dir.join("smabar.json"),
        r#"{"id":"crashy","name":"Crashy","version":"0.1.0","protocolVersion":1,
            "runtime":"exec","command":["python3","crash.py"],
            "tiles":[{"id":"w","name":"W"}]}"#,
    )
    .expect("write manifest");
    fs::write(
        plugin_dir.join("crash.py"),
        "import sys\nprint(\"boom-stdout\")\nprint(\"boom-stderr\", file=sys.stderr)\nsys.exit(1)\n",
    )
    .expect("write script");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();

    // Added, Starting, then the first failure.
    loop {
        if let PluginEvent::Status {
            status: PluginStatus::Failed,
            ..
        } = next_event(&mut events).await
        {
            break;
        }
    }
    supervisor.shutdown_all().await;

    let log = fs::read_to_string(paths.logs_dir().join("plugin-crashy.log"))
        .expect("plugin log file exists");
    assert!(log.contains("boom-stdout"), "missing stdout capture: {log}");
    assert!(log.contains("boom-stderr"), "missing stderr capture: {log}");
}

#[tokio::test]
async fn invalid_manifest_surfaces_as_failed_status() {
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("broken");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), "{ not json").expect("write manifest");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();

    match next_event(&mut events).await {
        PluginEvent::Status {
            plugin_id,
            status,
            error,
        } => {
            assert_eq!(plugin_id, "broken");
            assert_eq!(status, PluginStatus::Failed);
            let error = error.expect("failed status carries an error");
            assert!(error.contains("smabar.json"), "error: {error}");
        }
        other => panic!("expected failed status, got {other:?}"),
    }

    assert!(matches!(
        supervisor.dispatch_action("broken", "w", "x", None).await,
        Err(PluginError::UnknownPlugin { .. })
    ));
}

#[tokio::test]
async fn python_plugin_without_entry_file_fails_fast_and_recovers() {
    // Regression: plugin_write_file wrote smabar.json first; the watcher
    // started the plugin and `uv run` failed 5 times ("No such file") until
    // the entry script arrived. The supervisor must fail without spawning.
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("halfway");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        plugin_dir.join("smabar.json"),
        r#"{"id":"halfway","name":"Halfway","version":"0.1.0","protocolVersion":1,
            "runtime":"python","entry":"plugin.py",
            "tiles":[{"id":"w","name":"W"}]}"#,
    )
    .expect("write manifest");

    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();

    // Failed immediately — no Added/Starting, so no spawn/backoff loop ran.
    match next_event(&mut events).await {
        PluginEvent::Status {
            plugin_id,
            status,
            error,
        } => {
            assert_eq!(plugin_id, "halfway");
            assert_eq!(status, PluginStatus::Failed);
            let error = error.expect("failed status carries an error");
            assert!(error.contains("\"plugin.py\""), "error: {error}");
            assert!(
                error.contains("plugin_write_file(path=\"plugin.py\")"),
                "the error names the fix: {error}"
            );
        }
        other => panic!("expected failed status, got {other:?}"),
    }
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        supervisor.dispatch_action("halfway", "w", "x", None).await,
        Err(PluginError::UnknownPlugin { .. })
    ));

    // Once the entry script exists, the retry path (folder watcher and
    // plugin_reload both end in start_plugin) registers the plugin.
    fs::write(plugin_dir.join("plugin.py"), "print('hi')\n").expect("write entry");
    supervisor
        .restart("halfway")
        .await
        .expect("restart after entry exists");
    match next_event(&mut events).await {
        PluginEvent::Added { plugin_id, .. } => assert_eq!(plugin_id, "halfway"),
        other => panic!("expected Added once the entry exists, got {other:?}"),
    }
    supervisor.shutdown_all().await;
}

/// A plugin that spawns a long-running grandchild and records its pid.
///
/// The grandchild is what a real plugin's CLI call looks like to the OS, and
/// what a plain child kill used to leave running forever.
#[cfg(unix)]
const SPAWNER_SCRIPT: &str = r#"
import json, os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
kid = subprocess.Popen([sys.executable, "-c", "import time\ntime.sleep(600)"])
with open(os.path.join(here, "kid.pid"), "w") as handle:
    handle.write(str(kid.pid))

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method in ("initialize", "ping"):
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
        sys.exit(0)
"#;

/// Process groups are a unix concept; Windows keeps the direct-child kill
/// until its Job Object support lands (M7).
#[cfg(unix)]
#[tokio::test]
async fn stopping_a_plugin_kills_the_children_it_spawned() {
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("spawner");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        plugin_dir.join("smabar.json"),
        r#"{"id":"spawner","name":"Spawner","version":"1","protocolVersion":1,
            "runtime":"exec","command":["python3","main.py"],
            "tiles":[{"id":"w","name":"W"}]}"#,
    )
    .expect("write manifest");
    fs::write(plugin_dir.join("main.py"), SPAWNER_SCRIPT).expect("write script");

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

    let pid_file = plugin_dir.join("kid.pid");
    let kid: i32 = fs::read_to_string(&pid_file)
        .expect("the plugin records its child's pid")
        .trim()
        .parse()
        .expect("a pid");
    assert!(process_alive(kid), "the grandchild should be running");

    supervisor.shutdown_all().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert!(
        !process_alive(kid),
        "the grandchild outlived its plugin — stopping a plugin must take its \
         whole process group down, or every reload leaks a process"
    );
}

/// Whether `pid` still exists (signal 0 checks without delivering anything).
#[cfg(unix)]
fn process_alive(pid: i32) -> bool {
    // SAFETY: signal 0 performs the permission/existence check only.
    unsafe { libc::kill(pid, 0) == 0 }
}
