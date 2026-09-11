use super::catalog::{CatalogError, ItemKind, embedded_key, verify_and_parse};
use super::testing::{FIXTURE_CATALOG, FIXTURE_SIGNATURE};

/// The embedded key is the key the deployed store signs with. A signature
/// over fixed bytes never expires, so this stays green until the key changes
/// — which is exactly the moment this test has to fail.
#[test]
fn the_live_fixture_verifies_against_the_embedded_key() {
    let key = embedded_key().expect("embedded key parses");
    let listing = verify_and_parse(FIXTURE_CATALOG, FIXTURE_SIGNATURE, &key).expect("verifies");
    assert_eq!(listing.schema, 1);
    assert!(listing.plugin("hello").is_some());
    assert_eq!(
        listing.plugin("hello").expect("hello").common.path,
        "plugins/hello"
    );
}

#[test]
fn one_flipped_byte_is_rejected_before_parsing() {
    let key = embedded_key().expect("key");
    let mut tampered = FIXTURE_CATALOG.to_vec();
    let position = tampered
        .iter()
        .position(|byte| *byte == b'0')
        .expect("a zero digit");
    tampered[position] = b'1';
    let error = verify_and_parse(&tampered, FIXTURE_SIGNATURE, &key).expect_err("rejected");
    assert!(matches!(error, CatalogError::Signature(_)), "{error}");

    // Even syntactically valid JSON is never parsed without a signature.
    let error = verify_and_parse(b"{}", FIXTURE_SIGNATURE, &key).expect_err("rejected");
    assert!(matches!(error, CatalogError::Signature(_)), "{error}");
}

#[test]
fn a_signature_from_another_key_is_rejected() {
    let key = embedded_key().expect("key");
    // Same layout, different key id.
    let other = FIXTURE_SIGNATURE.replacen("RUQ016+I83QhYg", "RUQAAAAAAAAAAA", 1);
    let error = verify_and_parse(FIXTURE_CATALOG, &other, &key).expect_err("rejected");
    assert!(matches!(error, CatalogError::Signature(_)), "{error}");
    let error = verify_and_parse(FIXTURE_CATALOG, "garbage", &key).expect_err("rejected");
    assert!(matches!(error, CatalogError::Signature(_)), "{error}");
}

#[test]
fn unknown_fields_and_missing_defaults_are_tolerated() {
    let raw = r#"{
      "schema": 1, "generatedAt": "2026-09-03T00:00:00Z", "futureTop": 1,
      "items": [{
        "kind": "theme", "id": "nord", "name": "Nord", "version": "1.0.0",
        "author": {"login": "octo", "url": "https://github.com/octo", "avatar": "x"},
        "repo": {"id": 5, "url": "https://github.com/octo/t", "nameWithOwner": "octo/t"},
        "path": "themes/nord.json", "updatedAt": "2026-09-03T00:00:00Z",
        "detailSha256": "00", "source": {"commit": "c", "ref": "main", "fileUrl": "u", "sha256": "s", "extra": true}
      }],
      "blocklist": [{"kind": "theme", "id": "nord", "reason": "test"}]
    }"#;
    let listing: super::catalog::Listing = serde_json::from_str(raw).expect("parses");
    let theme = listing.theme("nord").expect("theme");
    assert_eq!(theme.common.requires.os, vec!["linux", "windows", "macos"]);
    assert!(theme.common.description.is_empty());
    assert_eq!(
        listing.block_reason(ItemKind::Theme, "nord", "1.0.0"),
        Some("test")
    );
    assert_eq!(
        listing.block_reason(ItemKind::Plugin, "nord", "1.0.0"),
        None
    );
}

#[test]
fn a_versioned_block_only_matches_that_version() {
    let key = embedded_key().expect("key");
    let mut listing = verify_and_parse(FIXTURE_CATALOG, FIXTURE_SIGNATURE, &key).expect("verifies");
    listing.blocklist.push(super::catalog::BlockEntry {
        kind: ItemKind::Plugin,
        id: "hello".to_string(),
        reason: "pilot".to_string(),
        version: Some("0.1.0".to_string()),
    });
    assert_eq!(
        listing.block_reason(ItemKind::Plugin, "hello", "0.1.0"),
        Some("pilot")
    );
    assert_eq!(
        listing.block_reason(ItemKind::Plugin, "hello", "0.1.1"),
        None
    );
}

#[test]
fn a_newer_schema_is_refused() {
    let raw = br#"{"schema": 2, "generatedAt": "x"}"#;
    // Bypass the signature: schema handling is what is under test here.
    let listing: Result<super::catalog::Listing, _> = serde_json::from_slice(raw);
    assert_eq!(listing.expect("parses").schema, 2);
    let key = embedded_key().expect("key");
    let error = verify_and_parse(raw, FIXTURE_SIGNATURE, &key).expect_err("unsigned");
    assert!(matches!(error, CatalogError::Signature(_)));
}
