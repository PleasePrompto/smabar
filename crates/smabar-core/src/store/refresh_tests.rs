use std::fs;

use super::catalog::ItemKind;
use super::receipts::{PluginReceipt, Receipts};
use super::refresh::cache_files_exist;
use super::testing::{
    CATALOG_URL, FIXTURE_CATALOG, FakeFetcher, FakeResponse, SIGNATURE_URL, serve_fixture, service,
};
use super::view::CatalogState;
use super::{StoreError, StoreService};

fn hello_receipt(service: &StoreService, version: &str) -> PluginReceipt {
    let entry = service
        .overview()
        .entries
        .into_iter()
        .find(|entry| entry.id == "hello")
        .expect("hello listed");
    PluginReceipt {
        repo_id: 1,
        name_with_owner: entry.repo.name_with_owner,
        path: entry.path,
        version: version.to_string(),
        commit: entry.commit,
        tree_oid: "t".repeat(40),
        installed_digest: "d".to_string(),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    }
}

#[tokio::test]
async fn a_verified_catalog_is_cached_and_served_fresh() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, Some("\"abc\""));
    let (_dir, paths, service) = service(fetcher.clone()).await;
    assert_eq!(service.overview().catalog_state, CatalogState::Unavailable);

    let overview = service.refresh().await;
    assert_eq!(overview.catalog_state, CatalogState::Fresh);
    assert_eq!(overview.entries.len(), 2);
    assert!(overview.last_error.is_none());
    assert!(cache_files_exist(&paths));

    // The second refresh is conditional and a 304 keeps everything.
    fetcher.respond(CATALOG_URL, FakeResponse::NotModified);
    let overview = service.refresh().await;
    assert_eq!(overview.catalog_state, CatalogState::Fresh);
    assert_eq!(overview.entries.len(), 2);
    let requests = fetcher.requests();
    assert_eq!(requests[0], (CATALOG_URL.to_string(), None));
    assert_eq!(requests[1].0, SIGNATURE_URL);
    assert_eq!(
        requests[2],
        (CATALOG_URL.to_string(), Some("\"abc\"".to_string()))
    );
}

#[tokio::test]
async fn an_unchanged_body_behind_a_new_etag_needs_no_signature_fetch() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, Some("\"strong\""));
    let (_dir, _paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    fetcher.body(CATALOG_URL, FIXTURE_CATALOG, Some("W/\"strong\""));
    service.refresh().await;
    let signature_fetches = fetcher
        .requests()
        .iter()
        .filter(|(url, _)| url == SIGNATURE_URL)
        .count();
    assert_eq!(signature_fetches, 1);
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_catalog_as_stale() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    fetcher.respond(CATALOG_URL, FakeResponse::Offline);

    let overview = service.refresh().await;
    assert_eq!(overview.catalog_state, CatalogState::Stale);
    assert_eq!(overview.entries.len(), 2);
    assert!(
        overview
            .last_error
            .expect("error")
            .contains("no connection")
    );
    assert!(cache_files_exist(&paths));
}

#[tokio::test]
async fn tampered_bytes_never_replace_the_verified_catalog() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    let cached = fs::read(paths.store_catalog_file()).expect("cached");

    let mut tampered = FIXTURE_CATALOG.to_vec();
    let last = tampered.len() - 2;
    tampered[last] = b' ';
    fetcher.body(CATALOG_URL, &tampered, None);
    let overview = service.refresh().await;
    assert_eq!(overview.catalog_state, CatalogState::Stale);
    assert!(overview.last_error.expect("error").contains("signature"));
    assert_eq!(
        fs::read(paths.store_catalog_file()).expect("cached"),
        cached
    );
    assert_eq!(overview.entries.len(), 2);
}

#[tokio::test]
async fn a_cached_catalog_with_a_broken_signature_is_discarded_at_start() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (dir, paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    drop(service);
    fs::write(
        paths.store_catalog_signature_file(),
        "untrusted comment: x\nAAAA\ntrusted comment: y\nBBBB\n",
    )
    .expect("corrupt");

    let paths2 = crate::config::SmabarPaths::new(dir.path().to_path_buf());
    let key = super::catalog::embedded_key().expect("key");
    assert!(super::refresh::load_cached(&paths2, &key).is_none());
    assert!(!cache_files_exist(&paths2));
}

#[tokio::test]
async fn the_blocklist_switches_an_installed_plugin_off_and_only_a_newer_catalog_lifts_it() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    fs::create_dir_all(paths.plugins_dir().join("hello")).expect("plugin dir");
    let mut receipts = Receipts::default();
    receipts
        .plugins
        .insert("hello".to_string(), hello_receipt(&service, "0.1.0"));
    super::receipts::save(&paths, &receipts).expect("save");

    let blocked = String::from_utf8_lossy(FIXTURE_CATALOG).replacen(
        "\"blocklist\":[]",
        "\"blocklist\":[{\"kind\":\"plugin\",\"id\":\"hello\",\"reason\":\"pilot\",\"version\":\"0.1.0\"}]",
        1,
    );
    assert_ne!(
        blocked.as_bytes(),
        FIXTURE_CATALOG,
        "the fixture must carry an empty blocklist"
    );
    let listing_bytes = blocked.into_bytes();
    // Unsigned test catalog: swap the service's listing directly.
    let listing: super::catalog::Listing = serde_json::from_slice(&listing_bytes).expect("parses");
    {
        let mut state = crate::util::lock_unpoisoned(&service.inner.state);
        state.listing = Some(listing);
    }
    super::refresh::reconcile_blocklist(&service.inner).await;

    let config = service.inner.config.current();
    assert!(config.plugins_deactivated.contains(&"hello".to_string()));
    let receipt = super::receipts::load(&paths).plugins["hello"].clone();
    assert!(receipt.deactivated_by_store);
    assert_eq!(receipt.blocked.as_ref().expect("blocked").reason, "pilot");
    let entry = service
        .overview()
        .entries
        .into_iter()
        .find(|e| e.id == "hello")
        .expect("hello");
    assert_eq!(
        entry
            .installed
            .expect("installed")
            .blocked
            .expect("blocked")
            .reason,
        "pilot"
    );

    // The same catalog again (or an older one) changes nothing.
    {
        let mut state = crate::util::lock_unpoisoned(&service.inner.state);
        let mut listing: super::catalog::Listing =
            serde_json::from_slice(FIXTURE_CATALOG).expect("parses");
        listing.generated_at = "2026-01-01T00:00:00Z".to_string();
        state.listing = Some(listing);
    }
    super::refresh::reconcile_blocklist(&service.inner).await;
    assert!(
        service
            .inner
            .config
            .current()
            .plugins_deactivated
            .contains(&"hello".to_string())
    );

    // A newer catalog without the block lifts it and switches the plugin back on.
    {
        let mut state = crate::util::lock_unpoisoned(&service.inner.state);
        let mut listing: super::catalog::Listing =
            serde_json::from_slice(FIXTURE_CATALOG).expect("parses");
        listing.generated_at = "2030-01-01T00:00:00Z".to_string();
        state.listing = Some(listing);
    }
    super::refresh::reconcile_blocklist(&service.inner).await;
    assert!(
        !service
            .inner
            .config
            .current()
            .plugins_deactivated
            .contains(&"hello".to_string())
    );
    let receipt = super::receipts::load(&paths).plugins["hello"].clone();
    assert!(receipt.blocked.is_none());
    assert!(!receipt.deactivated_by_store);
}

#[tokio::test]
async fn a_users_own_deactivation_survives_a_lifted_block() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    fs::create_dir_all(paths.plugins_dir().join("hello")).expect("plugin dir");
    crate::plugins::set_plugin_active(&service.inner.config, "hello", false).expect("deactivate");
    let mut receipts = Receipts::default();
    let mut receipt = hello_receipt(&service, "0.1.0");
    receipt.blocked = Some(super::receipts::BlockedState {
        reason: "old".to_string(),
        version: None,
        catalog_generated_at: "2020-01-01T00:00:00Z".to_string(),
    });
    receipts.plugins.insert("hello".to_string(), receipt);
    super::receipts::save(&paths, &receipts).expect("save");

    super::refresh::reconcile_blocklist(&service.inner).await;
    assert!(
        super::receipts::load(&paths).plugins["hello"]
            .blocked
            .is_none()
    );
    assert!(
        service
            .inner
            .config
            .current()
            .plugins_deactivated
            .contains(&"hello".to_string())
    );
}

#[tokio::test]
async fn receipts_without_a_folder_are_swept() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher.clone()).await;
    let mut receipts = Receipts::default();
    receipts
        .plugins
        .insert("gone".to_string(), hello_receipt_static());
    super::receipts::save(&paths, &receipts).expect("save");
    fs::create_dir_all(paths.store_backup_dir("gone")).expect("backup");

    service.refresh().await;
    assert!(super::receipts::load(&paths).plugins.is_empty());
    assert!(!paths.store_backup_dir("gone").exists());
}

#[tokio::test]
async fn catalog_refresh_preserves_backups_and_receipts_while_recovery_is_pending() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher).await;
    let mut receipts = Receipts::default();
    receipts
        .plugins
        .insert("gone".to_string(), hello_receipt_static());
    super::receipts::save(&paths, &receipts).expect("save");
    let backup = paths.store_backup_dir("gone");
    fs::create_dir_all(&backup).expect("backup");
    fs::write(backup.join("smabar.json"), "only remaining plugin code").expect("backup code");
    // Even a damaged journal must preserve the evidence needed to recover.
    fs::write(paths.store_journal_file(), "unreadable journal").expect("journal");

    assert_eq!(service.refresh().await.catalog_state, CatalogState::Fresh);
    assert_eq!(
        super::receipts::load(&paths).plugins.get("gone"),
        Some(&hello_receipt_static())
    );
    assert_eq!(
        fs::read_to_string(backup.join("smabar.json")).expect("retained backup"),
        "only remaining plugin code"
    );
    assert_eq!(
        fs::read_to_string(paths.store_journal_file()).expect("retained journal"),
        "unreadable journal"
    );
}

#[tokio::test]
async fn catalog_refresh_does_not_sweep_plugin_receipts_during_an_install() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, paths, service) = service(fetcher).await;
    let mut receipts = Receipts::default();
    receipts
        .plugins
        .insert("gone".to_string(), hello_receipt_static());
    super::receipts::save(&paths, &receipts).expect("save");
    fs::create_dir_all(paths.store_backup_dir("gone")).expect("backup");
    let _install = service.inner.install_lock.lock().await;
    assert!(!paths.store_journal_file().exists());

    assert_eq!(service.refresh().await.catalog_state, CatalogState::Fresh);
    assert_eq!(
        super::receipts::load(&paths).plugins.get("gone"),
        Some(&hello_receipt_static())
    );
    assert!(paths.store_backup_dir("gone").is_dir());
}

fn hello_receipt_static() -> PluginReceipt {
    PluginReceipt {
        repo_id: 1,
        name_with_owner: "x/y".to_string(),
        path: ".".to_string(),
        version: "0.1.0".to_string(),
        commit: "c".repeat(40),
        tree_oid: "t".repeat(40),
        installed_digest: "d".to_string(),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    }
}

#[tokio::test]
async fn details_are_verified_against_the_listed_digest_and_memoized() {
    let fetcher = FakeFetcher::new();
    serve_fixture(&fetcher, None);
    let (_dir, _paths, service) = service(fetcher.clone()).await;
    service.refresh().await;
    let detail_url = "https://store.test/items/plugin/hello.json";
    let listing: super::catalog::Listing = serde_json::from_slice(FIXTURE_CATALOG).expect("parses");
    let expected = listing
        .plugin("hello")
        .expect("hello")
        .common
        .detail_sha256
        .clone();

    fetcher.body(
        detail_url,
        br##"{"kind":"plugin","id":"hello","version":"0.1.0","readme":"# Hello"}"##,
        None,
    );
    let error = service
        .detail(ItemKind::Plugin, "hello")
        .await
        .expect_err("wrong digest");
    assert!(
        matches!(error, StoreError::DigestMismatch { .. }),
        "{error}"
    );

    // Find bytes that hash to the listed digest: the live detail file itself
    // is not in the fixture, so build the document the store would serve and
    // check the mismatch path only. A matching document is exercised by
    // pointing the listing at our own digest.
    let body =
        br##"{"kind":"plugin","id":"hello","version":"0.1.0","readme":"# Hello","releases":[]}"##;
    let digest = super::catalog::sha256_hex(body);
    assert_ne!(digest, expected);
    {
        let mut state = crate::util::lock_unpoisoned(&service.inner.state);
        let listing = state.listing.as_mut().expect("listing");
        for entry in &mut listing.items {
            if let super::catalog::Entry::Plugin(plugin) = entry
                && plugin.common.id == "hello"
            {
                plugin.common.detail_sha256 = digest.clone();
            }
        }
    }
    fetcher.body(detail_url, body, None);
    let detail = service
        .detail(ItemKind::Plugin, "hello")
        .await
        .expect("detail");
    assert_eq!(detail.readme.as_deref(), Some("# Hello"));
    assert_eq!(detail.readme_html.as_deref(), Some("<h1>Hello</h1>\n"));
    assert_eq!(detail.entry.id, "hello");
    service
        .detail(ItemKind::Plugin, "hello")
        .await
        .expect("memoized");
    let detail_fetches = fetcher
        .requests()
        .iter()
        .filter(|(url, _)| url == detail_url)
        .count();
    assert_eq!(detail_fetches, 2);

    let error = service
        .detail(ItemKind::Theme, "hello")
        .await
        .expect_err("not a theme");
    assert!(matches!(error, StoreError::Unknown { .. }));
}
