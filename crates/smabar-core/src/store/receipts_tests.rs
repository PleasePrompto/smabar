use std::fs;

use crate::config::SmabarPaths;

use super::receipts::{
    PluginReceipt, Receipts, folder_digest, forget_plugin, is_plugin_modified, load, save,
};

fn receipt() -> PluginReceipt {
    PluginReceipt {
        repo_id: 1,
        name_with_owner: "octo/plugins".to_string(),
        path: "plugins/hello".to_string(),
        version: "0.1.0".to_string(),
        commit: "c".repeat(40),
        tree_oid: "t".repeat(40),
        installed_digest: String::new(),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    }
}

#[test]
fn receipts_roundtrip_atomically_and_tolerate_unknown_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = SmabarPaths::new(dir.path().to_path_buf());
    let mut receipts = Receipts::default();
    receipts.plugins.insert("hello".to_string(), receipt());
    save(&paths, &receipts).expect("save");
    let loaded = load(&paths);
    assert_eq!(loaded.plugins.get("hello"), Some(&receipt()));
    let leftovers: Vec<_> = fs::read_dir(paths.store_dir())
        .expect("store dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec!["installed.json".to_string()]);

    let raw = fs::read_to_string(paths.store_receipts_file()).expect("read");
    let with_extra = raw.replacen("\"schema\": 1", "\"schema\": 1, \"future\": {\"x\": 1}", 1);
    fs::write(paths.store_receipts_file(), with_extra).expect("write");
    assert!(load(&paths).plugins.contains_key("hello"));

    fs::write(paths.store_receipts_file(), "{not json").expect("write");
    assert!(load(&paths).plugins.is_empty());
}

#[test]
fn forgetting_drops_the_receipt_and_the_backup_but_nothing_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = SmabarPaths::new(dir.path().to_path_buf());
    let mut receipts = Receipts::default();
    receipts.plugins.insert("hello".to_string(), receipt());
    receipts.plugins.insert("other".to_string(), receipt());
    save(&paths, &receipts).expect("save");
    fs::create_dir_all(paths.store_backup_dir("hello")).expect("backup");
    fs::create_dir_all(paths.store_backup_dir("other")).expect("backup");

    assert!(forget_plugin(&paths, "hello"));
    assert!(!forget_plugin(&paths, "hello"));
    assert!(!paths.store_backup_dir("hello").exists());
    assert!(paths.store_backup_dir("other").is_dir());
    assert!(load(&paths).plugins.contains_key("other"));
}

#[test]
fn the_digest_notices_edits_but_not_running_the_plugin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = SmabarPaths::new(dir.path().to_path_buf());
    let plugin = paths.plugins_dir().join("hello");
    fs::create_dir_all(plugin.join("lib")).expect("mkdir");
    fs::write(plugin.join("smabar.json"), "{}").expect("write");
    fs::write(plugin.join("lib/util.py"), "x = 1\n").expect("write");
    let installed_digest = folder_digest(&plugin).expect("digest");
    let receipt = PluginReceipt {
        installed_digest,
        ..receipt()
    };
    assert!(!is_plugin_modified(&paths, "hello", &receipt));

    // What a plugin does when it runs: bytecode caches and dot files.
    fs::create_dir_all(plugin.join("__pycache__")).expect("mkdir");
    fs::write(plugin.join("__pycache__/util.cpython-312.pyc"), "bytecode").expect("write");
    fs::write(plugin.join(".state"), "x").expect("write");
    assert!(!is_plugin_modified(&paths, "hello", &receipt));

    fs::write(plugin.join("lib/util.py"), "x = 2\n").expect("write");
    assert!(is_plugin_modified(&paths, "hello", &receipt));
}
