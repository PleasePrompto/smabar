use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::plugins::PluginStatus;

use super::receipts;
use super::testing::{
    CRASHING_SCRIPT, FakeFetcher, RUNNING_SCRIPT, TestSigner, serve_plugin, service_with_key,
};
use super::view::{InstallPhase, Origin};
use super::{StoreError, StoreService};

const NOW: &str = "2026-09-03T12:00:00Z";
const LATER: &str = "2026-09-04T12:00:00Z";

fn no_progress() -> impl Fn(super::InstallProgress) + Send + Sync {
    |_| {}
}

async fn wait_running(service: &StoreService, id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if service
            .inner
            .supervisor
            .plugin_infos()
            .iter()
            .any(|info| info.id == id && info.status == PluginStatus::Running)
        {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{id} never ran");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_fresh_install_runs_and_leaves_a_receipt_behind() {
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
    let phases = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&phases);
    let mut events = service.subscribe();

    let outcome = service
        .install_plugin("demo", "1.0.0", false, &move |progress| {
            crate::util::lock_unpoisoned(&seen).push(progress.phase);
        })
        .await
        .expect("install");

    assert_eq!(outcome.status, PluginStatus::Running, "{outcome:?}");
    assert!(outcome.settled);
    assert!(outcome.replaced_version.is_none());
    let receipt = receipts::load(&paths).plugins["demo"].clone();
    assert_eq!(receipt.version, "1.0.0");
    assert_eq!(receipt.commit, super::testing::TEST_COMMIT);
    assert!(!receipts::is_plugin_modified(&paths, "demo", &receipt));
    assert!(
        fs::read_dir(paths.store_staging_dir())
            .map(|entries| entries.count() == 0)
            .unwrap_or(true)
    );
    assert!(!paths.store_journal_file().exists());
    let phases = crate::util::lock_unpoisoned(&phases).clone();
    assert_eq!(phases.first(), Some(&InstallPhase::Downloading));
    assert_eq!(phases.last(), Some(&InstallPhase::Done));
    assert!(phases.contains(&InstallPhase::Verifying));
    let entry = service
        .overview()
        .entries
        .into_iter()
        .find(|entry| entry.id == "demo")
        .expect("entry");
    assert_eq!(entry.installed.expect("installed").origin, Origin::Store);
    assert!(service.overview().pending.is_none());
    let mut reasons = Vec::new();
    while let Ok(event) = events.try_recv() {
        reasons.push(event.reason);
    }
    assert!(reasons.contains(&super::ChangeReason::Install));
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn an_update_keeps_data_backs_up_the_old_version_and_respects_local_edits() {
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
    service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect("install");
    wait_running(&service, "demo").await;
    fs::write(paths.plugin_data_dir("demo").join("notes.txt"), "keep me").expect("data");

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.1.0",
        RUNNING_SCRIPT,
        LATER,
        |_| {},
    );
    let entry = service
        .refresh()
        .await
        .entries
        .into_iter()
        .find(|entry| entry.id == "demo")
        .expect("entry");
    assert_eq!(entry.update.as_ref().expect("update").to_version, "1.1.0");

    let outcome = service
        .install_plugin("demo", "1.1.0", false, &no_progress())
        .await
        .expect("update");
    assert_eq!(outcome.replaced_version.as_deref(), Some("1.0.0"));
    assert_eq!(outcome.status, PluginStatus::Running, "{outcome:?}");
    assert!(paths.store_backup_dir("demo").join("smabar.json").is_file());
    assert!(
        fs::read_to_string(paths.store_backup_dir("demo").join("smabar.json"))
            .expect("backup")
            .contains("1.0.0")
    );
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("demo").join("notes.txt")).expect("data"),
        "keep me"
    );
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.1.0");
    wait_running(&service, "demo").await;
    assert_eq!(
        fs::read_to_string(paths.plugin_data_dir("demo").join("started-by")).expect("marker"),
        "1.1.0"
    );

    // A local edit blocks the next update until confirmed.
    fs::write(paths.plugins_dir().join("demo").join("main.py"), "edited").expect("edit");
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.2.0",
        RUNNING_SCRIPT,
        "2026-09-05T12:00:00Z",
        |_| {},
    );
    let error = service
        .install_plugin("demo", "1.2.0", false, &no_progress())
        .await
        .expect_err("modified");
    assert!(
        matches!(error, StoreError::LocallyModified { .. }),
        "{error}"
    );
    let outcome = service
        .install_plugin("demo", "1.2.0", true, &no_progress())
        .await
        .expect("confirmed");
    assert_eq!(outcome.replaced_version.as_deref(), Some("1.1.0"));
    assert_eq!(
        fs::read_to_string(paths.store_backup_dir("demo").join("main.py")).expect("backup"),
        "edited"
    );
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_version_that_fails_to_start_is_rolled_back() {
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
    service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect("install");
    wait_running(&service, "demo").await;

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.1.0",
        CRASHING_SCRIPT,
        LATER,
        |_| {},
    );
    let error = service
        .install_plugin("demo", "1.1.0", false, &no_progress())
        .await
        .expect_err("crash");
    assert!(
        matches!(
            error,
            StoreError::StartFailed {
                rolled_back: true,
                ..
            }
        ),
        "{error}"
    );
    assert!(
        fs::read_to_string(paths.plugins_dir().join("demo").join("smabar.json"))
            .expect("manifest")
            .contains("1.0.0")
    );
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.0.0");
    wait_running(&service, "demo").await;
    assert!(!paths.store_staging_dir().join("demo.failed").exists());
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_fresh_install_that_fails_to_start_is_removed_again() {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.0.0",
        CRASHING_SCRIPT,
        NOW,
        |_| {},
    );
    let (_dir, paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;

    let error = service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect_err("crash");
    assert!(
        matches!(
            error,
            StoreError::StartFailed {
                rolled_back: true,
                ..
            }
        ),
        "{error}"
    );
    assert!(!paths.plugins_dir().join("demo").exists());
    assert!(receipts::load(&paths).plugins.is_empty());
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_deactivated_plugin_is_updated_but_stays_off() {
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
    service
        .install_plugin("demo", "1.0.0", false, &no_progress())
        .await
        .expect("install");
    wait_running(&service, "demo").await;
    crate::plugins::set_plugin_active(&service.inner.config, "demo", false).expect("deactivate");
    tokio::time::sleep(Duration::from_millis(800)).await;

    serve_plugin(
        &fetcher,
        &signer,
        "demo",
        "1.1.0",
        RUNNING_SCRIPT,
        LATER,
        |_| {},
    );
    let outcome = service
        .install_plugin("demo", "1.1.0", false, &no_progress())
        .await
        .expect("update");
    assert_eq!(outcome.status, PluginStatus::Deactivated, "{outcome:?}");
    assert_eq!(receipts::load(&paths).plugins["demo"].version, "1.1.0");
    assert!(
        service
            .inner
            .config
            .current()
            .plugins_deactivated
            .contains(&"demo".to_string())
    );
    service.inner.supervisor.shutdown_all().await;
}
