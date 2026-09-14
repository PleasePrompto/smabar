use std::fs;

use super::receipts;
use super::testing::{CATALOG_URL, FakeFetcher, SIGNATURE_URL, TestSigner, service_with_key};
use super::{StoreError, StoreService};

const THEME: &str = r##"{
  "meta": {"name": "Nord Test", "version": "1.0.0", "author": "octo"},
  "--sb-accent": "#88c0d0"
}
"##;

fn serve_theme(
    fetcher: &FakeFetcher,
    signer: &TestSigner,
    name: &str,
    version: &str,
    body: &str,
    edit: impl FnOnce(&mut serde_json::Value),
) {
    let commit = super::testing::TEST_COMMIT;
    let path = format!("themes/{name}.json");
    let file_url = super::fetch::expected_file_url(super::testing::TEST_REPO, commit, &path);
    let mut listing = serde_json::json!({
        "schema": 1, "generatedAt": "2026-09-03T12:00:00Z",
        "items": [{
            "kind": "theme", "id": name, "name": "Nord Test", "version": version,
            "author": {"login": "octo", "url": "https://github.com/octo"},
            "repo": {"id": 42, "url": "https://github.com/octo/demo", "nameWithOwner": super::testing::TEST_REPO},
            "path": path, "updatedAt": "2026-09-03T12:00:00Z", "detailSha256": "0",
            "source": {"commit": commit, "ref": "main", "fileUrl": file_url, "sha256": super::catalog::sha256_hex(body.as_bytes())}
        }],
        "blocklist": []
    });
    edit(&mut listing);
    let bytes = serde_json::to_vec(&listing).expect("json");
    fetcher.body(
        SIGNATURE_URL,
        signer.sign(&bytes, "catalog-v1.json").as_bytes(),
        None,
    );
    fetcher.body(CATALOG_URL, &bytes, None);
    fetcher.body(&file_url, body.as_bytes(), None);
}

async fn setup() -> (
    tempfile::TempDir,
    crate::config::SmabarPaths,
    StoreService,
    std::sync::Arc<FakeFetcher>,
    TestSigner,
) {
    let signer = TestSigner::new();
    let fetcher = FakeFetcher::new();
    let (dir, paths, service) = service_with_key(fetcher.clone(), signer.public_key()).await;
    (dir, paths, service, fetcher, signer)
}

#[tokio::test]
async fn a_theme_is_written_through_the_import_path_with_a_receipt() {
    let (_dir, paths, service, fetcher, signer) = setup().await;
    serve_theme(&fetcher, &signer, "nord-test", "1.0.0", THEME, |_| {});

    let outcome = service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect("install");
    assert_eq!(outcome.name, "nord-test");
    assert!(!outcome.active);
    let file = paths.themes_dir().join("nord-test.json");
    assert!(file.is_file());
    let receipt = receipts::load(&paths).themes["nord-test"].clone();
    assert_eq!(receipt.version, "1.0.0");
    assert!(!receipts::is_theme_modified(&paths, "nord-test", &receipt));
    assert!(crate::themes::available_themes(&paths).contains(&"nord-test".to_string()));
    let entry = service
        .overview()
        .entries
        .into_iter()
        .find(|entry| entry.id == "nord-test")
        .expect("entry");
    assert!(entry.installed.is_some());
    assert!(!entry.installable);

    let original = fs::read(&file).expect("theme bytes");
    let original_receipts = fs::read(paths.store_receipts_file()).expect("receipt bytes");
    service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect_err("already installed");
    assert_eq!(fs::read(&file).expect("theme unchanged"), original);
    assert_eq!(
        fs::read(paths.store_receipts_file()).expect("receipt unchanged"),
        original_receipts
    );

    fs::write(&file, "{\"--sb-accent\": \"#000000\"}").expect("edit");
    let entry = service
        .overview()
        .entries
        .into_iter()
        .find(|entry| entry.id == "nord-test")
        .expect("entry");
    assert!(entry.installed.expect("installed").modified);
    let edited = fs::read(&file).expect("edited theme");
    service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect_err("no update for edited theme");
    assert_eq!(fs::read(&file).expect("edits preserved"), edited);

    // Deleting through the theme path drops the receipt.
    crate::themes::io::delete_theme(&paths, "nord-test").expect("delete");
    assert!(receipts::load(&paths).themes.is_empty());
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn unreadable_receipts_are_preserved_without_importing_the_theme() {
    let (_dir, paths, service, fetcher, signer) = setup().await;
    serve_theme(&fetcher, &signer, "nord-test", "1.0.0", THEME, |_| {});
    fs::create_dir_all(paths.store_dir()).expect("store dir");
    let original = b"{\"plugins\": {\"precious\": damaged receipt data";
    fs::write(paths.store_receipts_file(), original).expect("receipts");

    let error = service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect_err("unreadable receipts must prevent import");
    assert!(matches!(error, StoreError::Io { .. }), "{error}");
    assert_eq!(
        fs::read(paths.store_receipts_file()).expect("retained receipts"),
        original
    );
    assert!(!paths.themes_dir().join("nord-test.json").exists());
    assert!(!paths.store_staging_dir().join("nord-test.json").exists());
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn theme_install_matches_the_overview_for_updates_and_downgrades() {
    let (_dir, paths, service, fetcher, signer) = setup().await;
    serve_theme(&fetcher, &signer, "nord-test", "1.0.0", THEME, |_| {});
    service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect("initial install");
    let changed = THEME.replace("#88c0d0", "#123456");
    for (version, body, installable) in [
        ("1.0.0", changed.as_str(), true),
        ("2.0.0", changed.as_str(), true),
        ("1.0.0", THEME, false),
    ] {
        serve_theme(&fetcher, &signer, "nord-test", version, body, |_| {});
        let overview = service.refresh().await;
        let entry = overview
            .entries
            .iter()
            .find(|entry| entry.id == "nord-test")
            .expect("theme");
        assert_eq!(entry.installable, installable);
        let file = paths.themes_dir().join("nord-test.json");
        let before = fs::read(&file).expect("installed file");
        let result = service.install_theme("nord-test", version).await;
        if installable {
            assert_eq!(result.expect("update").version, version);
        } else {
            assert!(matches!(result, Err(StoreError::NoUpdate { .. })));
            assert_eq!(fs::read(&file).expect("file unchanged"), before);
        }
    }
    service.inner.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn wrong_bytes_bundled_names_and_local_files_are_refused() {
    let (_dir, paths, service, fetcher, signer) = setup().await;
    serve_theme(&fetcher, &signer, "nord-test", "1.0.0", THEME, |listing| {
        listing["items"][0]["source"]["sha256"] = serde_json::json!("0".repeat(64));
    });
    let error = service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect_err("digest");
    assert!(
        matches!(error, StoreError::DigestMismatch { .. }),
        "{error}"
    );
    assert!(!paths.themes_dir().join("nord-test.json").exists());

    serve_theme(&fetcher, &signer, "default", "1.0.0", THEME, |_| {});
    let error = service
        .install_theme("default", "1.0.0")
        .await
        .expect_err("bundled");
    assert!(matches!(error, StoreError::BundledTheme { .. }), "{error}");

    serve_theme(&fetcher, &signer, "nord-test", "1.0.0", THEME, |_| {});
    fs::create_dir_all(paths.themes_dir()).expect("themes dir");
    fs::write(paths.themes_dir().join("nord-test.json"), THEME).expect("local theme");
    let error = service
        .install_theme("nord-test", "1.0.0")
        .await
        .expect_err("local");
    assert!(matches!(error, StoreError::LocalTheme { .. }), "{error}");
    service.inner.supervisor.shutdown_all().await;
}
