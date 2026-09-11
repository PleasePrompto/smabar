//! Deactivation: a plugin the user switched off must not run — not at boot,
//! not when its folder is written to, and not on an explicit reload.
//!
//! Same runtime discipline as `tests.rs`: current-thread tokio, so events are
//! seen deterministically.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{
    PluginError, PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions, set_plugin_active,
};

/// A plugin that answers the handshake and then idles, so "is it running?"
/// is a real question rather than a race with its own exit.
const SCRIPT: &str = r#"
import json
import sys

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method in ("initialize", "ping"):
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": {}}) + "\n")
        sys.stdout.flush()
    elif method == "shutdown":
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": {}}) + "\n")
        sys.stdout.flush()
        sys.exit(0)
"#;

fn manifest(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","name":"Idle {id}","version":"0.1.0","protocolVersion":1,
            "runtime":"exec","command":["python3","main.py"],
            "tiles":[{{"id":"w","name":"W"}}]}}"#
    )
}

/// Installs an idle plugin under `id` and returns its folder.
fn install(paths: &SmabarPaths, id: &str) -> std::path::PathBuf {
    let dir = paths.plugins_dir().join(id);
    fs::create_dir_all(&dir).expect("create plugin dir");
    fs::write(dir.join("main.py"), SCRIPT).expect("write script");
    fs::write(dir.join("smabar.json"), manifest(id)).expect("write manifest");
    dir
}

fn watcher(paths: &SmabarPaths) -> Arc<ConfigWatcher> {
    Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"))
}

async fn supervisor(paths: &SmabarPaths, config: Arc<ConfigWatcher>) -> PluginSupervisor {
    PluginSupervisor::start(
        paths.clone(),
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await
}

/// Waits until `plugin_infos` reports `expected` for `id`, or gives up.
async fn await_status(supervisor: &PluginSupervisor, id: &str, expected: PluginStatus) {
    for _ in 0..200 {
        if supervisor
            .plugin_infos()
            .iter()
            .any(|info| info.id == id && info.status == expected)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "plugin {id} never reached {expected:?}; saw {:?}",
        supervisor
            .plugin_infos()
            .iter()
            .map(|info| (info.id.clone(), info.status))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_deactivated_plugin_is_not_started_at_boot_but_stays_visible() {
    let (_dir, paths) = temp_paths();
    install(&paths, "idler");
    let config = watcher(&paths);
    set_plugin_active(&config, "idler", false).expect("deactivate before boot");

    let supervisor = supervisor(&paths, config).await;

    // No process: the plugin never registers, so there is nothing to talk to.
    assert!(
        matches!(
            supervisor.dispatch_action("idler", "w", "x", None).await,
            Err(PluginError::UnknownPlugin { .. })
        ),
        "a deactivated plugin must have no lifecycle task"
    );
    assert!(
        supervisor.current_plugins().is_empty(),
        "a deactivated plugin contributes no tiles to the bar"
    );

    // But it must remain findable, or the user could never switch it back on.
    let infos = supervisor.plugin_infos();
    let info = infos
        .iter()
        .find(|info| info.id == "idler")
        .expect("a deactivated plugin stays listed as installed");
    assert_eq!(info.status, PluginStatus::Deactivated);
    assert!(info.error.is_none());
    assert_eq!(
        info.manifest
            .as_ref()
            .map(|manifest| manifest.name.as_str()),
        Some("Idle idler"),
        "its manifest is kept, so the settings panel can show its name"
    );
    assert_eq!(
        info.dir.as_deref(),
        Some(paths.plugins_dir().join("idler").as_path())
    );
    assert_eq!(
        supervisor.plugin_dir("idler"),
        Some(paths.plugins_dir().join("idler")),
        "the folder still resolves — deleting it later has to find it"
    );

    // Its files are untouched: this is reversible, not a removal.
    assert!(paths.plugins_dir().join("idler/smabar.json").is_file());
}

#[tokio::test]
async fn the_folder_watcher_does_not_resurrect_a_deactivated_plugin() {
    let (_dir, paths) = temp_paths();
    let dir = install(&paths, "idler");
    let config = watcher(&paths);
    let supervisor = supervisor(&paths, Arc::clone(&config)).await;
    await_status(&supervisor, "idler", PluginStatus::Running).await;

    set_plugin_active(&config, "idler", false).expect("deactivate");
    await_status(&supervisor, "idler", PluginStatus::Deactivated).await;

    // A write to the plugin's own code folder is exactly what normally
    // restarts it — the whole point of deactivation is that it does not.
    fs::write(dir.join("main.py"), format!("{SCRIPT}\n# edited\n")).expect("edit the entry script");
    fs::write(dir.join("smabar.json"), manifest("idler")).expect("rewrite the manifest");
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(
        matches!(
            supervisor.dispatch_action("idler", "w", "x", None).await,
            Err(PluginError::UnknownPlugin { .. })
        ),
        "editing a deactivated plugin's code must not start it"
    );
    let infos = supervisor.plugin_infos();
    let info = infos
        .iter()
        .find(|info| info.id == "idler")
        .expect("listed");
    assert_eq!(info.status, PluginStatus::Deactivated);
}

#[tokio::test]
async fn deactivating_stops_the_process_and_activating_starts_it_again() {
    let (_dir, paths) = temp_paths();
    install(&paths, "idler");
    let config = watcher(&paths);
    let supervisor = supervisor(&paths, Arc::clone(&config)).await;
    await_status(&supervisor, "idler", PluginStatus::Running).await;
    supervisor
        .dispatch_action("idler", "w", "x", None)
        .await
        .expect("a running plugin accepts actions");

    let mut events = supervisor.subscribe_events();
    assert!(
        set_plugin_active(&config, "idler", false).expect("deactivate"),
        "the first deactivation changes the config"
    );
    // The tiles have to leave the bar: nothing is behind them any more.
    loop {
        if let PluginEvent::Removed { plugin_id } = next_event(&mut events).await {
            assert_eq!(plugin_id, "idler");
            break;
        }
    }
    await_status(&supervisor, "idler", PluginStatus::Deactivated).await;
    assert!(matches!(
        supervisor.dispatch_action("idler", "w", "x", None).await,
        Err(PluginError::UnknownPlugin { .. })
    ));
    // Repeating it is a no-op rather than a second stop.
    assert!(
        !set_plugin_active(&config, "idler", false).expect("deactivate again"),
        "already deactivated must not rewrite the config"
    );

    set_plugin_active(&config, "idler", true).expect("activate");
    await_status(&supervisor, "idler", PluginStatus::Running).await;
    supervisor
        .dispatch_action("idler", "w", "x", None)
        .await
        .expect("the reactivated plugin runs again");
    assert!(
        supervisor.current_plugins().iter().any(|m| m.id == "idler"),
        "its tiles are back on the bar"
    );
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn reloading_a_deactivated_plugin_is_refused_with_the_way_out() {
    let (_dir, paths) = temp_paths();
    install(&paths, "idler");
    let config = watcher(&paths);
    let supervisor = supervisor(&paths, Arc::clone(&config)).await;
    await_status(&supervisor, "idler", PluginStatus::Running).await;
    set_plugin_active(&config, "idler", false).expect("deactivate");
    await_status(&supervisor, "idler", PluginStatus::Deactivated).await;

    let error = supervisor
        .restart("idler")
        .await
        .expect_err("a restart must not silently do nothing");
    assert!(matches!(error, PluginError::Deactivated { .. }));
    let message = error.to_string();
    assert!(message.contains("pluginsDeactivated"), "{message}");
}

/// Hiding is the other axis and must leave the process completely alone —
/// that separation is the whole reason both settings exist.
#[tokio::test]
async fn hiding_a_tile_leaves_its_plugin_running() {
    let (_dir, paths) = temp_paths();
    install(&paths, "idler");
    let config = watcher(&paths);
    let supervisor = supervisor(&paths, Arc::clone(&config)).await;
    await_status(&supervisor, "idler", PluginStatus::Running).await;

    let mut hidden = config.current();
    hidden.plugins_hidden.push("plugin:idler:w".to_string());
    config.apply(hidden).expect("hide the tile");
    tokio::time::sleep(Duration::from_millis(300)).await;

    supervisor
        .dispatch_action("idler", "w", "x", None)
        .await
        .expect("hiding a tile must not stop its plugin");
    let infos = supervisor.plugin_infos();
    let info = infos
        .iter()
        .find(|info| info.id == "idler")
        .expect("listed");
    assert_eq!(
        info.status,
        PluginStatus::Running,
        "a hidden tile's plugin keeps running (it may still push popups)"
    );
    assert!(
        config.current().plugins_deactivated.is_empty(),
        "hiding must never write the deactivation list"
    );
    supervisor.shutdown_all().await;
}
