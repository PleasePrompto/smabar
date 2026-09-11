use std::fs;

use super::receipts;
use super::testing::{FakeFetcher, RUNNING_SCRIPT, TestSigner, serve_plugin, service_with_key};
use super::{InstallProgress, StoreError};

const NOW: &str = "2026-09-03T12:00:00Z";
const LATER: &str = "2026-09-04T12:00:00Z";

fn no_progress() -> impl Fn(InstallProgress) + Send + Sync {
    |_| {}
}

#[tokio::test]
async fn pending_recovery_blocks_a_new_install_before_network_or_staging() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    let (_dir, paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;
    fs::create_dir_all(paths.store_staging_dir()).unwrap();
    fs::write(paths.store_staging_dir().join("demo"), "recover me").unwrap();
    fs::write(paths.store_journal_file(), "unreadable but protected").unwrap();
    let requests = fetcher.requests();
    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("recovery is pending"));
    assert_eq!(fetcher.requests(), requests);
    assert_eq!(
        fs::read(paths.store_staging_dir().join("demo")).unwrap(),
        b"recover me"
    );
    assert_eq!(
        fs::read(paths.store_journal_file()).unwrap(),
        b"unreadable but protected"
    );
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn receipt_write_failure_after_install_leaves_a_recoverable_journal() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        NOW,
        |_| {},
    );
    let (_dir, paths, service) = service_with_key(fetcher, signer.public_key()).await;
    let obstruction = paths
        .store_receipts_file()
        .parent()
        .unwrap()
        .join(".installed.json.tmp");
    let error = service
        .install_plugin("demo", "1.0.0", false, &|progress| {
            if progress.phase == super::view::InstallPhase::Installing {
                fs::create_dir(&obstruction).unwrap();
            }
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("save the store receipt"));
    assert!(paths.store_journal_file().is_file());
    assert!(paths.plugins_dir().join("demo").is_dir());
    service.inner.supervisor.shutdown_all().await;
    fs::remove_dir(obstruction).unwrap();
    super::recover(&paths).unwrap();
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.0.0");
    assert!(!paths.store_journal_file().exists());
}

#[tokio::test]
async fn refused_before_anything_is_written() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        NOW,
        |_| {},
    );
    let (_dir, paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;

    let error = service
        .install_plugin("demo", "0.9.0", false, &no_progress())
        .await
        .expect_err("version");
    assert!(
        matches!(error, StoreError::VersionChanged { .. }),
        "{error}"
    );
    let error = service
        .install_plugin("nope", "1.0.0", false, &no_progress())
        .await
        .expect_err("unknown");
    assert!(matches!(error, StoreError::Unknown { .. }), "{error}");
    let error = service
        .install_plugin("clock", "1.0.0", false, &no_progress())
        .await
        .expect_err("base");
    assert!(matches!(error, StoreError::Unknown { .. }), "{error}");

    // A folder without a receipt is the user's own.
    fs::create_dir_all(paths.plugins_dir().join("demo")).expect("user plugin");
    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("user plugin");
    assert!(matches!(error, StoreError::UserPlugin { .. }), "{error}");
    fs::remove_dir_all(paths.plugins_dir().join("demo")).expect("cleanup");

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        LATER,
        |listing| {
            listing["items"][0]["requires"]["os"] = serde_json::json!(["plan9"]);
        },
    );
    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("os");
    assert!(matches!(error, StoreError::Incompatible { .. }), "{error}");

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        LATER,
        |listing| {
            listing["blocklist"] =
                serde_json::json!([{"kind": "plugin", "id": "demo", "reason": "pilot"}]);
        },
    );
    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("blocked");
    assert!(matches!(error, StoreError::Blocked { .. }), "{error}");

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        LATER,
        |listing| {
            listing["items"][0]["source"]["archiveUrl"] =
                serde_json::json!("https://evil.example/x.zip");
        },
    );
    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("contract");
    assert!(matches!(error, StoreError::Contract { .. }), "{error}");

    assert!(!paths.plugins_dir().join("demo").exists());
    assert!(receipts::load(&paths).plugins.is_empty());
    let archive_fetches = fetcher
        .requests()
        .iter()
        .filter(|(url, _)| url.ends_with(".zip"))
        .count();
    assert_eq!(archive_fetches, 0);
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_tree_mismatch_installs_nothing() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        NOW,
        |listing| {
            listing["items"][0]["source"]["treeOid"] = serde_json::json!("f".repeat(40));
        },
    );
    let (_dir, paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;

    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("tree");
    assert!(matches!(error, StoreError::TreeMismatch { .. }), "{error}");
    assert!(!paths.plugins_dir().join("demo").exists());
    assert!(!paths.store_staging_dir().join("demo").exists());
    assert!(receipts::load(&paths).plugins.is_empty());
    assert!(!paths.store_journal_file().exists());
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn an_unreachable_store_refuses_to_install_from_the_cache() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        RUNNING_SCRIPT,
        NOW,
        |_| {},
    );
    let (_dir, _paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;
    service.refresh().await;
    fetcher.respond(
        super::testing::CATALOG_URL,
        super::testing::FakeResponse::Offline,
    );

    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("offline");
    assert!(matches!(error, StoreError::StoreUnreachable(_)), "{error}");
    service.inner.supervisor.shutdown_all().await;
}
