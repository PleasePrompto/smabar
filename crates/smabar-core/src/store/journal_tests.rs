use std::fs;

use super::receipts;

fn interrupted(paths: &crate::config::SmabarPaths) -> super::journal::Journal {
    let backup = paths.store_backup_dir("demo");
    fs::create_dir_all(&backup).expect("backup");
    fs::write(backup.join("smabar.json"), "old code").expect("old code");
    fs::create_dir_all(paths.plugin_data_dir("demo")).expect("data dir");
    fs::write(paths.plugin_data_dir("demo").join("notes"), "irreplaceable").expect("data");
    fs::create_dir_all(paths.store_staging_dir()).expect("staging");
    fs::write(paths.store_staging_dir().join("retained"), "staged code").expect("stage");
    let old = receipts::PluginReceipt {
        repo_id: 1,
        name_with_owner: "octo/demo".into(),
        path: ".".into(),
        version: "1.0.0".into(),
        commit: "c".repeat(40),
        tree_oid: "t".repeat(40),
        installed_digest: receipts::folder_digest(&backup).expect("digest"),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    };
    let transaction = super::journal::Journal {
        id: "demo".into(),
        had_previous: true,
        receipt: Some(receipts::PluginReceipt {
            version: "1.1.0".into(),
            installed_digest: "new code digest".into(),
            ..old.clone()
        }),
        installed_digest: None,
        previous_receipt: Some(old),
    };
    super::journal::write(paths, &transaction).expect("journal");
    transaction
}

fn assert_protected(paths: &crate::config::SmabarPaths) {
    assert!(super::journal::pending(paths));
    assert!(crate::plugins::sweep_orphaned_data(paths).is_empty());
    assert_eq!(
        fs::read(paths.plugin_data_dir("demo").join("notes")).unwrap(),
        b"irreplaceable"
    );
    assert!(paths.store_staging_dir().join("retained").is_file());
}

#[tokio::test]
async fn failed_restore_preserves_data_and_refuses_removal_until_retry_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::config::SmabarPaths::new(root.path().to_owned());
    let config = crate::config::ConfigWatcher::spawn(paths.clone()).unwrap();
    let transaction = interrupted(&paths);
    fs::create_dir_all(paths.plugins_dir()).unwrap();
    let plugin = paths.plugins_dir().join("demo");
    fs::write(&plugin, "blocking file").unwrap();
    let journal = fs::read(paths.store_journal_file()).unwrap();

    assert!(super::recover(&paths).is_err());
    assert_protected(&paths);
    assert!(paths.store_backup_dir("demo").is_dir());
    assert!(super::journal::write(&paths, &transaction).is_err());
    assert_eq!(fs::read(paths.store_journal_file()).unwrap(), journal);
    let other = paths.plugins_dir().join("other");
    fs::create_dir_all(&other).unwrap();
    assert!(crate::plugins::remove_plugin(&paths, &config, "other", &other).is_err());
    assert!(other.is_dir());
    assert_protected(&paths);

    fs::remove_file(plugin).unwrap();
    assert_eq!(super::recover(&paths).unwrap().as_deref(), Some("demo"));
    assert!(!super::journal::pending(&paths));
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.0.0");
    assert!(crate::plugins::sweep_orphaned_data(&paths).is_empty());
    assert_eq!(
        fs::read(paths.plugin_data_dir("demo").join("notes")).unwrap(),
        b"irreplaceable"
    );
    assert_eq!(super::recover(&paths).unwrap(), None);
}

#[test]
fn failed_receipt_save_keeps_restored_plugin_and_journal_for_retry() {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::config::SmabarPaths::new(root.path().to_owned());
    interrupted(&paths);
    // Block the atomic writer's temporary file, without relying on user permissions.
    let obstruction = paths
        .store_receipts_file()
        .parent()
        .unwrap()
        .join(".installed.json.tmp");
    fs::create_dir(&obstruction).unwrap();
    assert!(super::recover(&paths).is_err());
    assert!(paths.plugins_dir().join("demo").is_dir());
    assert_protected(&paths);
    fs::remove_dir(obstruction).unwrap();
    super::recover(&paths).unwrap();
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.0.0");
    assert!(!super::journal::pending(&paths));
}

#[test]
fn invalid_journal_missing_backup_and_unreadable_receipts_preserve_recovery() {
    for failure in ["journal", "id", "backup", "backup-file", "receipts"] {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::config::SmabarPaths::new(root.path().to_owned());
        let mut transaction = interrupted(&paths);
        match failure {
            "journal" => fs::write(paths.store_journal_file(), "broken JSON").unwrap(),
            "id" => {
                transaction.id = "../outside".into();
                fs::write(
                    paths.store_journal_file(),
                    serde_json::to_vec(&transaction).unwrap(),
                )
                .unwrap();
            }
            "backup" => fs::remove_dir_all(paths.store_backup_dir("demo")).unwrap(),
            "backup-file" => {
                fs::remove_dir_all(paths.store_backup_dir("demo")).unwrap();
                fs::write(paths.store_backup_dir("demo"), "not a plugin directory").unwrap();
            }
            "receipts" => fs::write(paths.store_receipts_file(), "broken JSON").unwrap(),
            _ => unreachable!(),
        }
        let journal = fs::read(paths.store_journal_file()).unwrap();
        assert!(super::recover(&paths).is_err(), "{failure}");
        assert_protected(&paths);
        assert_eq!(fs::read(paths.store_journal_file()).unwrap(), journal);
    }
}

#[test]
fn recovery_restores_or_completes_an_interrupted_swap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = crate::config::SmabarPaths::new(dir.path().to_path_buf());
    let plugin = paths.plugins_dir().join("demo");
    let backup = paths.store_backup_dir("demo");
    let make = |target: &std::path::Path, version: &str| {
        fs::create_dir_all(target).expect("dir");
        fs::write(target.join("smabar.json"), version).expect("file");
    };
    let old_receipt = receipts::PluginReceipt {
        repo_id: 1,
        name_with_owner: "octo/demo".to_string(),
        path: ".".to_string(),
        version: "1.0.0".to_string(),
        commit: "c".repeat(40),
        tree_oid: "t".repeat(40),
        installed_digest: "old".to_string(),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    };

    // Crash after the old folder moved away: the backup comes back.
    make(&backup, "1.0.0");
    make(&paths.store_staging_dir().join("demo"), "1.1.0");
    let new_digest =
        receipts::folder_digest(&paths.store_staging_dir().join("demo")).expect("digest");
    let new_receipt = receipts::PluginReceipt {
        version: "1.1.0".to_string(),
        installed_digest: new_digest.clone(),
        ..old_receipt.clone()
    };
    super::journal::write(
        &paths,
        &super::journal::Journal {
            id: "demo".to_string(),
            had_previous: true,
            receipt: Some(new_receipt.clone()),
            installed_digest: None,
            previous_receipt: Some(old_receipt.clone()),
        },
    )
    .expect("journal");
    assert_eq!(
        super::recover(&paths).expect("recovery").as_deref(),
        Some("demo")
    );
    assert_eq!(
        fs::read_to_string(plugin.join("smabar.json")).expect("restored"),
        "1.0.0"
    );
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.0.0");
    assert!(!paths.store_journal_file().exists());
    assert!(!paths.store_staging_dir().exists());

    // Crash after the new folder moved in: the receipt is completed.
    fs::remove_dir_all(&plugin).expect("reset");
    make(&plugin, "1.1.0");
    super::journal::write(
        &paths,
        &super::journal::Journal {
            id: "demo".to_string(),
            had_previous: true,
            receipt: Some(new_receipt),
            installed_digest: None,
            previous_receipt: Some(old_receipt),
        },
    )
    .expect("journal");
    super::recover(&paths).expect("recovery");
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.1.0");
    assert_eq!(super::recover(&paths).expect("already recovered"), None);
}
