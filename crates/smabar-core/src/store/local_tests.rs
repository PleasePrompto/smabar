use super::testing::{
    CRASHING_SCRIPT, FakeFetcher, RUNNING_SCRIPT, TestSigner, ZipEntry, service_with_key,
    test_manifest, zip_bytes,
};
use super::{
    StoreService,
    archive::{ArchiveLimits, extract_local_plugin},
};
use std::fs;

fn file(name: &str, bytes: impl Into<Vec<u8>>) -> ZipEntry {
    ZipEntry::File {
        name: name.into(),
        bytes: bytes.into(),
        mode: None,
        deflate: true,
    }
}
fn archive(prefix: &str, id: &str, version: &str, script: &str) -> Vec<u8> {
    zip_bytes(&[
        file(&format!("{prefix}smabar.json"), test_manifest(id, version)),
        file(
            &format!("{prefix}main.py"),
            script.replace("VERSION", version),
        ),
    ])
}
async fn install(
    service: &StoreService,
    path: &std::path::Path,
) -> Result<super::InstallOutcome, super::StoreError> {
    let preview = service.inspect_local_plugin(path.to_path_buf()).await?;
    service
        .install_local_plugin(
            path.to_path_buf(),
            &preview.archive_sha256,
            preview.previous_digest.as_deref(),
            &|_| {},
        )
        .await
}
#[test]
fn local_archives_accept_flat_or_wrapped_and_reject_ambiguous_or_unsafe_entries() {
    for prefix in ["", "download-name/"] {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("staged");
        extract_local_plugin(
            &archive(prefix, "demo", "1.0.0", RUNNING_SCRIPT),
            &dest,
            &ArchiveLimits::DEFAULT,
        )
        .expect("extract");
        assert!(dest.join("smabar.json").is_file());
        assert!(!dest.join("download-name").exists());
    }
    for entries in [
        vec![file("a/smabar.json", "{}"), file("b/smabar.json", "{}")],
        vec![file("a/b/smabar.json", "{}")],
        vec![file("smabar.json", "{}"), file("../escape", "bad")],
        vec![file("smabar.json", "{}"), file("/absolute", "bad")],
        vec![file("smabar.json", "{}"), file("CON.txt", "bad")],
        vec![file("smabar.json", "{}"), file("x", "a"), file("X", "b")],
        vec![
            file("smabar.json", "{}"),
            ZipEntry::Symlink {
                name: "link".into(),
                target: "../escape".into(),
            },
        ],
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            extract_local_plugin(
                &zip_bytes(&entries),
                &dir.path().join("stage"),
                &ArchiveLimits::DEFAULT
            )
            .is_err()
        );
        assert!(!dir.path().join("escape").exists());
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = ArchiveLimits {
        max_file_bytes: 1,
        ..ArchiveLimits::DEFAULT
    };
    assert!(
        extract_local_plugin(
            &archive("", "demo", "1.0.0", RUNNING_SCRIPT),
            dir.path(),
            &limits
        )
        .is_err()
    );
}
#[tokio::test]
async fn local_install_and_same_id_update_preserve_data_and_keep_no_store_receipt() {
    let signer = TestSigner::new();
    let (dir, paths, service) = service_with_key(FakeFetcher::new(), signer.public_key()).await;
    let zip = dir.path().join("plugin.zip");
    fs::write(&zip, archive("folder/", "demo", "1.0.0", RUNNING_SCRIPT)).expect("zip");
    assert_eq!(
        install(&service, &zip).await.expect("install").status,
        crate::plugins::PluginStatus::Running
    );
    fs::write(paths.plugin_data_dir("demo").join("notes"), "keep").expect("data");
    let mut config = service.inner.config.current();
    config.plugins_deactivated.push("demo".into());
    service
        .inner
        .config
        .apply(config.clone())
        .expect("deactivate");
    fs::write(
        &zip,
        archive(
            "",
            "demo",
            "1.0.0",
            &(RUNNING_SCRIPT.to_string() + "\n# new code"),
        ),
    )
    .expect("update zip");
    install(&service, &zip).await.expect("same version update");
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("demo").join("notes")).expect("data"),
        "keep"
    );
    assert_eq!(service.inner.config.current(), config);
    assert!(
        fs::read_to_string(paths.plugins_dir().join("demo/main.py"))
            .expect("new code")
            .contains("# new code")
    );
    assert!(paths.store_backup_dir("demo").join("main.py").is_file());
    assert!(!super::receipts::load(&paths).plugins.contains_key("demo"));
    assert!(!paths.store_journal_file().exists());
    service.inner.supervisor.shutdown_all().await;
}
#[tokio::test]
async fn changed_zip_and_existing_plugin_need_a_new_confirmation_and_base_ids_are_protected() {
    let signer = TestSigner::new();
    let (dir, paths, service) = service_with_key(FakeFetcher::new(), signer.public_key()).await;
    let zip = dir.path().join("plugin.zip");
    fs::write(&zip, archive("", "demo", "1.0.0", RUNNING_SCRIPT)).expect("zip");
    let preview = service
        .inspect_local_plugin(zip.clone())
        .await
        .expect("preview");
    fs::write(&zip, archive("", "demo", "2.0.0", RUNNING_SCRIPT)).expect("changed");
    assert!(
        service
            .install_local_plugin(zip.clone(), &preview.archive_sha256, None, &|_| {})
            .await
            .is_err()
    );
    assert!(!paths.plugins_dir().join("demo").exists());
    install(&service, &zip).await.expect("install");
    let preview = service
        .inspect_local_plugin(zip.clone())
        .await
        .expect("preview");
    fs::write(paths.plugins_dir().join("demo/user-edit"), "keep").expect("edit");
    assert!(
        service
            .install_local_plugin(
                zip.clone(),
                &preview.archive_sha256,
                preview.previous_digest.as_deref(),
                &|_| {}
            )
            .await
            .is_err()
    );
    fs::write(&zip, archive("", "clock", "1.0.0", RUNNING_SCRIPT)).expect("base zip");
    assert!(matches!(
        service.inspect_local_plugin(zip).await,
        Err(super::StoreError::BasePlugin { .. })
    ));
    service.inner.supervisor.shutdown_all().await;
}
#[tokio::test]
async fn failed_local_update_restores_old_code_and_preserves_user_data() {
    let signer = TestSigner::new();
    let (dir, paths, service) = service_with_key(FakeFetcher::new(), signer.public_key()).await;
    let zip = dir.path().join("plugin.zip");
    fs::write(&zip, archive("", "demo", "1.0.0", RUNNING_SCRIPT)).expect("zip");
    install(&service, &zip).await.expect("install");
    fs::write(paths.plugin_data_dir("demo").join("notes"), "keep").expect("notes");
    fs::write(&zip, archive("", "demo", "2.0.0", CRASHING_SCRIPT)).expect("broken update");
    assert!(matches!(
        install(&service, &zip).await,
        Err(super::StoreError::StartFailed {
            rolled_back: true,
            ..
        })
    ));
    let manifest = crate::plugins::PluginManifest::load(&paths.plugins_dir().join("demo"))
        .expect("restored manifest");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("demo").join("notes")).expect("data"),
        "keep"
    );
    assert!(!paths.store_journal_file().exists());
    service.inner.supervisor.shutdown_all().await;
}
#[tokio::test]
async fn invalid_python_entry_and_concurrent_install_do_not_publish_files() {
    let signer = TestSigner::new();
    let (dir, paths, service) = service_with_key(FakeFetcher::new(), signer.public_key()).await;
    let zip = dir.path().join("plugin.zip");
    let manifest = r#"{"id":"demo","name":"Demo","version":"1.0.0","protocolVersion":1,"runtime":"python","entry":"../escape.py","tiles":[{"id":"main","name":"Main"}]}"#;
    fs::write(&zip, zip_bytes(&[file("smabar.json", manifest)])).expect("zip");
    assert!(matches!(
        service.inspect_local_plugin(zip.clone()).await,
        Err(super::StoreError::EntryMissing { .. })
    ));
    let lock = service.inner.install_lock.lock().await;
    assert!(matches!(
        service.inspect_local_plugin(zip).await,
        Err(super::StoreError::Busy)
    ));
    assert!(!paths.plugins_dir().join("demo").exists());
    drop(lock);
    service.inner.supervisor.shutdown_all().await;
}
#[test]
fn recovery_finishes_local_install_without_fabricating_a_store_receipt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = crate::config::SmabarPaths::new(dir.path().into());
    let plugin = paths.plugins_dir().join("demo");
    fs::create_dir_all(&plugin).expect("plugin");
    fs::write(plugin.join("smabar.json"), test_manifest("demo", "2.0.0")).expect("manifest");
    super::journal::write(
        &paths,
        &super::journal::Journal {
            id: "demo".into(),
            had_previous: false,
            receipt: None,
            previous_receipt: None,
            installed_digest: Some(super::receipts::folder_digest(&plugin).expect("digest")),
        },
    )
    .expect("journal");
    super::recover(&paths).expect("recover");
    assert!(plugin.join("smabar.json").is_file());
    assert!(!super::receipts::load(&paths).plugins.contains_key("demo"));
    assert!(!paths.store_journal_file().exists());
}
