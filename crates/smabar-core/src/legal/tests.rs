use std::fs;

use super::render::render;
use super::{LICENSE, PRIVACY, TERMS, accept, is_accepted, parse_document, status};
use crate::config::SmabarPaths;

fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    (dir, paths)
}

fn write_acceptance(paths: &SmabarPaths, json: &str) {
    fs::create_dir_all(paths.base_dir()).expect("create base dir");
    fs::write(paths.legal_file(), json).expect("write legal.json");
}

/// `YYYY-MM-DD`, checked independently of the module's own validator.
fn assert_iso_date(value: Option<&str>) {
    let value = value.expect("a date");
    let parts: Vec<&str> = value.split('-').collect();
    assert_eq!(
        parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
        [4, 2, 2],
        "{value}"
    );
    assert!(
        parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit())),
        "{value}"
    );
}

#[test]
fn bundled_texts_parse_and_both_languages_share_one_version() {
    for document in [&TERMS.de, &TERMS.en, &PRIVACY.de, &PRIVACY.en] {
        assert!(!document.title.is_empty());
        assert!(document.html.contains("<h2>"), "{}", document.title);
        assert_iso_date(document.updated.as_deref());
    }
    assert_eq!(TERMS.de.updated, TERMS.en.updated);
    assert_eq!(PRIVACY.de.updated, PRIVACY.en.updated);
    assert_ne!(TERMS.de.html, TERMS.en.html);
    assert!(LICENSE.html.contains("PolyForm Shield"), "{}", LICENSE.html);
    assert_eq!(LICENSE.updated, None);
}

#[test]
fn a_broken_frontmatter_yields_no_version() {
    assert!(parse_document("## No frontmatter\n").is_err());
    assert!(parse_document("---\ntitle: T\nupdated: 2026-09-07\n").is_err());
    assert!(parse_document("---\nupdated: 2026-09-07\n---\nbody\n").is_err());
    assert!(parse_document("---\ntitle: T\n---\nbody\n").is_err());
    assert!(parse_document("---\ntitle: T\nupdated: 7.9.2026\n---\nbody\n").is_err());
    let document = parse_document(
        "---\r\ntitle: T\r\ndescription: a: b\r\nupdated: 2026-09-07\r\n---\r\n\r\n## Body\r\n",
    )
    .expect("valid");
    assert_eq!(document.title, "T");
    assert_eq!(document.updated.as_deref(), Some("2026-09-07"));
    assert_eq!(document.html.trim(), "<h2>Body</h2>");
}

#[test]
fn a_fresh_profile_requires_acceptance() {
    let (_dir, paths) = temp_paths();
    assert!(!is_accepted(&paths));
    let fresh = status(&paths, "en");
    assert!(fresh.required);
    assert_eq!(fresh.accepted_at, None);
    assert_eq!(
        Some(fresh.terms_version.as_str()),
        TERMS.en.updated.as_deref()
    );
    assert_eq!(
        Some(fresh.privacy_version.as_str()),
        PRIVACY.en.updated.as_deref()
    );
}

#[test]
fn accepting_persists_and_clears_the_requirement() {
    let (_dir, paths) = temp_paths();
    let accepted = accept(&paths, "en").expect("accept");
    assert!(!accepted.required);
    assert!(accepted.accepted_at.is_some());
    let json = fs::read_to_string(paths.legal_file()).expect("legal.json");
    assert!(json.contains("\"schema\": 1"), "{json}");
    assert!(
        json.contains(&format!("\"termsVersion\": \"{}\"", accepted.terms_version)),
        "{json}"
    );
    assert!(json.ends_with('\n'));
    assert!(is_accepted(&paths));
    assert!(!status(&paths, "de").required);
}

#[test]
fn an_older_accepted_version_reopens_the_gate_and_a_newer_one_does_not() {
    let (_dir, paths) = temp_paths();
    write_acceptance(
        &paths,
        r#"{"schema":1,"termsVersion":"2000-01-01","acceptedAt":5}"#,
    );
    assert!(!is_accepted(&paths));
    let outdated = status(&paths, "en");
    assert!(outdated.required);
    assert_eq!(outdated.accepted_at, Some(5));
    write_acceptance(
        &paths,
        r#"{"schema":1,"termsVersion":"2999-12-31","acceptedAt":6}"#,
    );
    assert!(is_accepted(&paths));
    assert!(!status(&paths, "en").required);
}

#[test]
fn an_unreadable_acceptance_counts_as_not_accepted() {
    let (_dir, paths) = temp_paths();
    write_acceptance(&paths, "{ not json");
    assert!(!is_accepted(&paths));
    assert!(status(&paths, "en").required);
    write_acceptance(&paths, r#"{"termsVersion":"soon","acceptedAt":1}"#);
    assert!(!is_accepted(&paths));
    // Accepting over the broken file repairs it.
    assert!(!accept(&paths, "en").expect("accept").required);
    assert!(is_accepted(&paths));
}

#[test]
fn accepting_reports_an_unwritable_profile() {
    let (dir, paths) = temp_paths();
    fs::write(dir.path().join("smabar"), b"not a directory").expect("occupy the base path");
    assert!(accept(&paths, "en").is_err());
    assert!(!is_accepted(&paths));
}

#[test]
fn language_falls_back_to_english() {
    let (_dir, paths) = temp_paths();
    let english = status(&paths, "en");
    let french = status(&paths, "fr");
    let german = status(&paths, "de");
    assert_eq!(french.terms.html, english.terms.html);
    assert_eq!(french.privacy.html, english.privacy.html);
    assert_ne!(german.terms.html, english.terms.html);
    assert_ne!(german.privacy.html, english.privacy.html);
    assert_eq!(german.license.html, english.license.html);
}

#[test]
fn render_drops_raw_html_and_keeps_its_text() {
    let html = render("<script>alert(1)</script>\n\nplain <b>bold</b> text\n");
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("alert(1)"), "{html}");
    assert!(html.contains("plain bold text"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
}

#[test]
fn render_keeps_only_https_links() {
    let html = render(
        "[ok](https://smabar.com/) [plain](http://smabar.com/) [js](javascript:alert(1)) [rel](../x) [anchor](#top) <https://polyformproject.org/>\n",
    );
    assert!(
        html.contains("<a href=\"https://smabar.com/\">ok</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"https://polyformproject.org/\">"),
        "{html}"
    );
    assert_eq!(html.matches("<a ").count(), 2, "{html}");
    for text in ["plain", "js", "rel", "anchor"] {
        assert!(html.contains(text), "{html}");
    }
    assert!(!html.contains("javascript:"), "{html}");
    assert!(!html.contains("http://"), "{html}");
}

#[test]
fn render_reduces_images_to_alt_text() {
    let html = render(
        "![logo](https://example.com/logo.png) and [![badge](x.png)](https://smabar.com/)\n",
    );
    assert!(!html.contains("<img"), "{html}");
    assert!(html.contains("logo"), "{html}");
    assert!(
        html.contains("<a href=\"https://smabar.com/\">badge</a>"),
        "{html}"
    );
}
