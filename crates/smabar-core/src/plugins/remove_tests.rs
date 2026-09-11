//! Deleting a plugin has to take everything with it — code, data AND log.
//! Leaving one of the three behind was the bug this feature exists to avoid.

use std::fs;
use std::sync::Arc;

use crate::config::{ConfigWatcher, SmabarPaths};
use crate::providers::ProviderHub;

use super::tests::{next_event, temp_paths};
use super::{
    PluginEvent, PluginStatus, PluginSupervisor, RemoveError, SupervisorOptions, remove_plugin,
    set_plugin_active,
};

const RUNNING_MANIFEST: &str = r#"{
  "id": "doomed",
  "name": "Doomed",
  "version": "0.1.0",
  "protocolVersion": 1,
  "runtime": "exec",
  "command": ["python3", "main.py"],
  "tiles": [{"id": "main", "name": "Main", "hasFlyout": false}]
}"#;

const RUNNING_SCRIPT: &str = r#"
import json
import pathlib
import sys

data = None
for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        data = pathlib.Path(message["params"]["dataDir"])
        data.mkdir(parents=True, exist_ok=True)
        (data / "running").write_text("yes")
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
    elif message.get("method") == "shutdown":
        (data / "shutdown").write_text("yes")
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
        sys.exit(0)
"#;

/// Installs a plugin folder plus the data directory and log file a plugin
/// would have produced by running.
fn install_with_leftovers(paths: &SmabarPaths, id: &str) -> std::path::PathBuf {
    let dir = paths.plugins_dir().join(id);
    fs::create_dir_all(&dir).expect("create plugin dir");
    fs::write(dir.join("smabar.json"), "{}").expect("write manifest");
    fs::create_dir_all(paths.plugin_data_dir(id)).expect("create data dir");
    fs::write(paths.plugin_data_dir(id).join("cache.db"), "rows").expect("write data");
    fs::create_dir_all(paths.logs_dir()).expect("create logs dir");
    fs::write(paths.logs_dir().join(format!("plugin-{id}.log")), "{}\n").expect("write log");
    fs::write(paths.logs_dir().join(format!("plugin-{id}.log.1")), "{}\n").expect("write rotation");
    dir
}

fn watcher(paths: &SmabarPaths) -> Arc<ConfigWatcher> {
    Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn config watcher"))
}

#[tokio::test]
async fn removal_never_sweeps_another_plugins_temporarily_missing_code() {
    let (_dir, paths) = temp_paths();
    let doomed = install_with_leftovers(&paths, "doomed");
    let updating = install_with_leftovers(&paths, "updating");
    let config = watcher(&paths);
    fs::rename(updating, paths.base_dir().join("swapping-code")).unwrap();
    remove_plugin(&paths, &config, "doomed", &doomed).unwrap();
    assert!(paths.plugin_data_dir("updating").join("cache.db").is_file());
    assert!(paths.logs_dir().join("plugin-updating.log").is_file());
    assert!(!paths.plugin_data_dir("doomed").exists());
}

#[tokio::test]
async fn supervised_removal_stops_the_process_before_sweeping_its_data() {
    let (_dir, paths) = temp_paths();
    let plugin_dir = paths.plugins_dir().join("doomed");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("smabar.json"), RUNNING_MANIFEST).expect("write manifest");
    fs::write(plugin_dir.join("main.py"), RUNNING_SCRIPT).expect("write script");
    let config = watcher(&paths);
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
            plugin_id,
            status: PluginStatus::Running,
            ..
        } = next_event(&mut events).await
            && plugin_id == "doomed"
        {
            break;
        }
    }

    let removal = supervisor.remove("doomed").await.expect("remove");

    assert_eq!(removal.dir, plugin_dir);
    assert!(supervisor.current_plugins().is_empty());
    assert!(!plugin_dir.exists());
    assert!(!paths.plugin_data_dir("doomed").exists());
    loop {
        if let PluginEvent::Removed { plugin_id } = next_event(&mut events).await
            && plugin_id == "doomed"
        {
            break;
        }
    }
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn removing_a_plugin_deletes_its_code_its_data_and_its_logs() {
    let (_dir, paths) = temp_paths();
    let dir = install_with_leftovers(&paths, "doomed");
    // A second plugin proves the sweep only takes the deleted one's things.
    let keeper = install_with_leftovers(&paths, "keeper");
    let config = watcher(&paths);

    let removal = remove_plugin(&paths, &config, "doomed", &dir).expect("remove");

    assert!(!dir.exists(), "the code folder is gone");
    assert!(
        !paths.plugin_data_dir("doomed").exists(),
        "the data directory is gone"
    );
    assert!(
        !paths.logs_dir().join("plugin-doomed.log").exists(),
        "the log file is gone"
    );
    assert!(
        !paths.logs_dir().join("plugin-doomed.log.1").exists(),
        "the rotated log is gone too"
    );
    assert!(removal.swept.contains(&"doomed".to_string()));
    assert!(!removal.deactivation_cleared, "it was not deactivated");

    assert!(keeper.exists(), "another plugin's code survives");
    assert!(
        paths.plugin_data_dir("keeper").join("cache.db").is_file(),
        "another plugin's data survives"
    );
    assert!(
        paths.logs_dir().join("plugin-keeper.log").is_file(),
        "another plugin's log survives"
    );
}

/// The settings block is kept on purpose: reinstalling the same id restores
/// the user's configuration instead of starting from scratch.
#[tokio::test]
async fn removing_keeps_the_settings_block_but_clears_the_deactivation() {
    let (_dir, paths) = temp_paths();
    let dir = install_with_leftovers(&paths, "doomed");
    let config = watcher(&paths);

    let mut current = config.current();
    current.plugins.insert(
        "doomed".to_string(),
        serde_json::json!({ "city": "Berlin" }),
    );
    config.apply(current).expect("store plugin settings");
    set_plugin_active(&config, "doomed", false).expect("deactivate before deleting");

    let removal = remove_plugin(&paths, &config, "doomed", &dir).expect("remove");

    assert!(
        removal.deactivation_cleared,
        "a deleted plugin must not stay on the deactivation list"
    );
    let after = config.current();
    assert!(
        after.plugins_deactivated.is_empty(),
        "otherwise reinstalling this id would produce a plugin that refuses to start"
    );
    assert_eq!(
        after.plugins.get("doomed"),
        Some(&serde_json::json!({ "city": "Berlin" })),
        "the settings block survives so a reinstall restores the configuration"
    );
}

#[tokio::test]
async fn removing_rejects_unknown_ids_and_bad_ids_without_touching_anything() {
    let (_dir, paths) = temp_paths();
    install_with_leftovers(&paths, "keeper");
    let config = watcher(&paths);

    let missing = paths.plugins_dir().join("ghost");
    let error = remove_plugin(&paths, &config, "ghost", &missing).expect_err("nothing installed");
    assert!(matches!(error, RemoveError::NotInstalled { .. }));

    let error = remove_plugin(
        &paths,
        &config,
        "Bad Id",
        &paths.plugins_dir().join("keeper"),
    )
    .expect_err("invalid id");
    assert!(matches!(error, RemoveError::InvalidId { .. }));
    assert!(
        paths.plugins_dir().join("keeper").exists(),
        "a rejected call must delete nothing"
    );
}

/// A deleted plugin leaves no trace in the id lists. They are inert once it
/// is gone, but they grow without bound — and a stale `pluginsHidden` entry
/// would bring the tile back already hidden if the same id is reinstalled,
/// with nothing in the UI to explain why.
#[tokio::test]
async fn removing_drops_the_plugin_tile_ids_from_the_order_lists() {
    let (_dir, paths) = temp_paths();
    let dir = install_with_leftovers(&paths, "doomed");
    install_with_leftovers(&paths, "keeper");
    let config = watcher(&paths);

    let mut current = config.current();
    current.plugin_order = vec![
        "plugin:keeper:main".to_string(),
        "plugin:doomed:one".to_string(),
        "plugin:doomed:two".to_string(),
    ];
    current.plugins_hidden = vec![
        "plugin:doomed:two".to_string(),
        "plugin:keeper:main".to_string(),
    ];
    config.apply(current).expect("seed the id lists");

    let removal = remove_plugin(&paths, &config, "doomed", &dir).expect("remove");

    assert_eq!(
        removal.order_entries_cleared,
        vec![
            "plugin:doomed:one".to_string(),
            "plugin:doomed:two".to_string()
        ],
        "both tile ids are reported, each once"
    );
    let after = config.current();
    assert_eq!(
        after.plugin_order,
        vec!["plugin:keeper:main".to_string()],
        "another plugin keeps its place in the order"
    );
    assert_eq!(
        after.plugins_hidden,
        vec!["plugin:keeper:main".to_string()],
        "and its hidden state"
    );
}

/// A plugin that was never in either list is removed just the same, and says
/// so rather than reporting a change it did not make.
#[tokio::test]
async fn removing_a_plugin_with_no_order_entries_reports_none() {
    let (_dir, paths) = temp_paths();
    let dir = install_with_leftovers(&paths, "doomed");
    let config = watcher(&paths);

    let removal = remove_plugin(&paths, &config, "doomed", &dir).expect("remove");

    assert!(removal.order_entries_cleared.is_empty());
    assert!(config.current().plugin_order.is_empty());
}

/// A Community Plugin's receipt and backup belong to its folder.
#[tokio::test]
async fn removal_drops_the_store_receipt_and_backup() {
    let (_dir, paths) = temp_paths();
    let dir = install_with_leftovers(&paths, "shop");
    let config = watcher(&paths);
    let mut receipts = crate::store::receipts::Receipts::default();
    receipts.plugins.insert(
        "shop".to_string(),
        crate::store::receipts::PluginReceipt {
            repo_id: 1,
            name_with_owner: "octo/shop".to_string(),
            path: ".".to_string(),
            version: "1.0.0".to_string(),
            commit: "c".repeat(40),
            tree_oid: "t".repeat(40),
            installed_digest: "d".to_string(),
            installed_at: 1,
            blocked: None,
            deactivated_by_store: false,
        },
    );
    crate::store::receipts::save(&paths, &receipts).expect("save receipts");
    fs::create_dir_all(paths.store_backup_dir("shop")).expect("backup");

    let removal = remove_plugin(&paths, &config, "shop", &dir).expect("remove");

    assert!(removal.store_receipt_cleared);
    assert!(!paths.store_backup_dir("shop").exists());
    assert!(crate::store::receipts::load(&paths).plugins.is_empty());
}
