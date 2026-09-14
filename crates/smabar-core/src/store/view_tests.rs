use std::collections::{BTreeMap, BTreeSet};

use semver::Version;

use super::catalog::{BlockEntry, ItemKind, Listing};
use super::compat::Incompatibility;
use super::receipts::{PluginReceipt, Receipts};
use super::testing::FIXTURE_CATALOG;
use super::view::{
    CatalogState, Origin, OverviewInput, PluginPresence, ThemePresence, build_overview,
};

fn listing() -> Listing {
    serde_json::from_slice(FIXTURE_CATALOG).expect("fixture parses")
}

fn receipt_for(listing: &Listing, id: &str) -> PluginReceipt {
    let plugin = listing.plugin(id).expect("listed");
    PluginReceipt {
        repo_id: plugin.common.repo.id,
        name_with_owner: plugin.common.repo.name_with_owner.clone(),
        path: plugin.common.path.clone(),
        version: plugin.common.version.clone(),
        commit: plugin.source.commit.clone(),
        tree_oid: plugin.source.tree_oid.clone(),
        installed_digest: "d".to_string(),
        installed_at: 1,
        blocked: None,
        deactivated_by_store: false,
    }
}

struct Scenario {
    listing: Listing,
    receipts: Receipts,
    plugins: BTreeMap<String, PluginPresence>,
    themes: BTreeMap<String, ThemePresence>,
    reserved: BTreeSet<String>,
    app: Version,
    host_os: Option<&'static str>,
}

impl Scenario {
    fn new() -> Self {
        Self {
            listing: listing(),
            receipts: Receipts::default(),
            plugins: BTreeMap::new(),
            themes: BTreeMap::new(),
            reserved: BTreeSet::from(["clock".to_string()]),
            app: Version::parse("0.1.1").expect("semver"),
            host_os: Some("linux"),
        }
    }

    fn overview(&self) -> super::view::StoreOverview {
        build_overview(OverviewInput {
            listing: Some(&self.listing),
            catalog_state: CatalogState::Fresh,
            fetched_at: Some(5),
            last_error: None,
            receipts: &self.receipts,
            plugins: &self.plugins,
            themes: &self.themes,
            reserved_ids: &self.reserved,
            app_version: &self.app,
            host_os: self.host_os,
            pending: None,
        })
    }

    fn entry(&self, id: &str) -> super::view::StoreEntry {
        self.overview()
            .entries
            .into_iter()
            .find(|entry| entry.id == id)
            .expect("entry")
    }
}

#[test]
fn a_fresh_listing_is_installable_and_carries_the_source_facts() {
    let scenario = Scenario::new();
    let hello = scenario.entry("hello");
    assert!(hello.installable);
    assert!(hello.installed.is_none());
    assert!(hello.incompatible.is_empty());
    assert_eq!(hello.path, "plugins/hello");
    assert_eq!(hello.commit.len(), 40);
    assert_eq!(hello.repo.name_with_owner, "PleasePrompto/smabar-plugins");
    assert_eq!(
        scenario.overview().generated_at.as_deref(),
        Some("2026-09-03T09:03:00Z")
    );
}

#[test]
fn media_is_optional_and_only_github_https_images_reach_clients() {
    let mut scenario = Scenario::new();
    let plain = serde_json::to_value(scenario.entry("hello")).expect("entry JSON");
    assert!(plain.get("icon").is_some());
    assert_eq!(plain["icon"], serde_json::Value::Null);
    assert_eq!(plain["screenshots"], serde_json::json!([]));

    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE_CATALOG).expect("fixture");
    let icon = "https://raw.githubusercontent.com/o/r/commit/icon.png";
    let screenshot = "https://user-images.githubusercontent.com/1/shot.png";
    let rejected = [
        "http://github.com/o/r/icon.png",
        "https://github.com.evil.test/icon.png",
        "https://evilgithubusercontent.com/icon.png",
        "https://user:password@github.com/icon.png",
        "https://github.com:8443/icon.png",
        "https://example.org/icon.png",
        "data:image/png;base64,AAAA",
        "file:///tmp/icon.png",
        "icon.png",
    ];
    for item in raw["items"].as_array_mut().expect("items") {
        item["icon"] = serde_json::json!(icon);
        item["screenshots"] = serde_json::json!(
            rejected
                .iter()
                .copied()
                .chain([screenshot])
                .collect::<Vec<_>>()
        );
    }
    scenario.listing = serde_json::from_value(raw.clone()).expect("media catalog");
    for entry in scenario.overview().entries {
        let response = serde_json::to_value(entry).expect("entry JSON");
        assert_eq!(response["icon"], icon);
        assert_eq!(response["screenshots"], serde_json::json!([screenshot]));
    }
    for url in rejected {
        for item in raw["items"].as_array_mut().expect("items") {
            item["icon"] = serde_json::json!(url);
            item["screenshots"] = serde_json::json!(vec![screenshot; 8]);
        }
        scenario.listing = serde_json::from_value(raw.clone()).expect("bad URL is only bad media");
        let response = serde_json::to_value(scenario.entry("hello")).expect("entry JSON");
        assert_eq!(response["icon"], serde_json::Value::Null);
        assert_eq!(
            response["screenshots"]
                .as_array()
                .expect("screenshots")
                .len(),
            6
        );
        assert_eq!(response["installable"], true);
    }
}

#[test]
fn an_installed_current_copy_is_neither_installable_nor_updatable() {
    let mut scenario = Scenario::new();
    let receipt = receipt_for(&scenario.listing, "hello");
    scenario
        .receipts
        .plugins
        .insert("hello".to_string(), receipt);
    scenario.plugins.insert(
        "hello".to_string(),
        PluginPresence {
            version: Some("0.1.0".to_string()),
            deactivated: false,
            modified: Some(false),
        },
    );
    let hello = scenario.entry("hello");
    let installed = hello.installed.expect("installed");
    assert_eq!(installed.origin, Origin::Store);
    assert!(!installed.modified);
    assert!(hello.update.is_none());
    assert!(!hello.installable);
}

#[test]
fn a_newer_listing_or_moved_content_offers_an_update() {
    let mut scenario = Scenario::new();
    let mut receipt = receipt_for(&scenario.listing, "hello");
    receipt.version = "0.0.9".to_string();
    scenario
        .receipts
        .plugins
        .insert("hello".to_string(), receipt.clone());
    scenario
        .plugins
        .insert("hello".to_string(), PluginPresence::default());
    let update = scenario.entry("hello").update.expect("update");
    assert_eq!(
        (update.from_version.as_str(), update.to_version.as_str()),
        ("0.0.9", "0.1.0")
    );
    assert!(!update.content_changed);
    assert!(scenario.entry("hello").installable);

    receipt.version = "0.1.0".to_string();
    receipt.tree_oid = "0".repeat(40);
    scenario
        .receipts
        .plugins
        .insert("hello".to_string(), receipt);
    let update = scenario.entry("hello").update.expect("content update");
    assert!(update.content_changed);
}

#[test]
fn a_folder_without_a_receipt_is_the_users_own() {
    let mut scenario = Scenario::new();
    scenario.plugins.insert(
        "hello".to_string(),
        PluginPresence {
            version: Some("9.9.9".to_string()),
            deactivated: true,
            modified: None,
        },
    );
    let hello = scenario.entry("hello");
    let installed = hello.installed.expect("installed");
    assert_eq!(installed.origin, Origin::Local);
    assert_eq!(installed.version, "9.9.9");
    assert!(installed.deactivated);
    assert_eq!(hello.incompatible, vec![Incompatibility::UserPlugin]);
    assert!(!hello.installable);
}

#[test]
fn incompatibilities_are_listed_in_full() {
    let mut scenario = Scenario::new();
    scenario.host_os = Some("windows");
    scenario.app = Version::parse("0.0.1").expect("semver");
    scenario.reserved.insert("hello".to_string());
    let plugin = scenario.listing.plugin("hello").expect("listed").clone();
    let mut edited = plugin;
    edited.common.requires.os = vec!["linux".to_string()];
    edited.common.requires.smabar = Some("0.1.0".to_string());
    scenario.listing.items = vec![super::catalog::Entry::Plugin(edited)];
    scenario.listing.blocklist.push(BlockEntry {
        kind: ItemKind::Plugin,
        id: "hello".to_string(),
        reason: "pilot".to_string(),
        version: None,
    });
    let hello = scenario.entry("hello");
    assert_eq!(
        hello.incompatible,
        vec![
            Incompatibility::Os,
            Incompatibility::MinSmabar,
            Incompatibility::BasePlugin,
            Incompatibility::Blocked
        ]
    );
    assert_eq!(hello.blocked.expect("blocked").reason, "pilot");
    assert!(!hello.installable);
}

#[test]
fn an_unavailable_catalog_yields_no_entries_but_keeps_the_facts() {
    let overview = build_overview(OverviewInput {
        listing: None,
        catalog_state: CatalogState::Unavailable,
        fetched_at: None,
        last_error: Some("offline".to_string()),
        receipts: &Receipts::default(),
        plugins: &BTreeMap::new(),
        themes: &BTreeMap::new(),
        reserved_ids: &BTreeSet::new(),
        app_version: &Version::parse("0.1.1").expect("semver"),
        host_os: None,
        pending: None,
    });
    assert!(overview.entries.is_empty());
    assert_eq!(overview.catalog_state, CatalogState::Unavailable);
    assert_eq!(overview.last_error.as_deref(), Some("offline"));
    assert_eq!(overview.app_version, "0.1.1");
}
