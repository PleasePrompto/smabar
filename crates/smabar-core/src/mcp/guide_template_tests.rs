//! The template plugin is what agents copy, so it has to practise every rule
//! the guide states: siblings in write order, a name on every icon-only
//! control, complete locales in both languages.

use std::collections::BTreeSet;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use super::guide_tools::{
    TEMPLATE_FILES, TEMPLATE_LOCALE_DE, TEMPLATE_LOCALE_EN, TEMPLATE_SCRIPT, TEMPLATE_VIEWS,
};
use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};

fn keys(locale: &str) -> BTreeSet<String> {
    let map: serde_json::Map<String, Value> =
        serde_json::from_str(locale).expect("a locale file is a flat JSON object");
    map.iter()
        .map(|(key, value)| {
            assert!(value.is_string(), "locale value of {key} must be a string");
            key.clone()
        })
        .collect()
}

/// Every quoted `folder.…` key literal the template's python reads, in
/// either quote style (the formatter picks one per string).
fn used_keys() -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for source in [TEMPLATE_SCRIPT, TEMPLATE_VIEWS] {
        for (index, _) in source.match_indices("folder.") {
            let quoted = index > 0 && matches!(&source[index - 1..index], "\"" | "'");
            let name: String = source[index + "folder.".len()..]
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect();
            if quoted && !name.is_empty() {
                used.insert(format!("folder.{name}"));
            }
        }
    }
    used
}

#[test]
fn the_template_is_served_in_write_order_with_the_manifest_last() {
    let paths: Vec<&str> = TEMPLATE_FILES.iter().map(|(path, _)| *path).collect();
    assert_eq!(paths.last(), Some(&"smabar.json"));
    assert!(paths.contains(&"plugin.py") && paths.contains(&"views.py"));
    assert!(
        paths.iter().position(|p| *p == "views.py") < paths.iter().position(|p| *p == "plugin.py"),
        "the sibling the entry script imports comes first"
    );
    assert!(TEMPLATE_SCRIPT.contains("import views"));
}

#[test]
fn icon_only_controls_in_the_template_carry_a_name_and_a_tooltip() {
    assert!(TEMPLATE_VIEWS.contains("aria-label=") && TEMPLATE_VIEWS.contains("title="));
    // The helper is the mechanism: every icon-only button goes through it.
    assert!(TEMPLATE_VIEWS.contains("icon_only"));
}

#[test]
fn the_template_locales_are_complete_in_both_languages() {
    let en = keys(TEMPLATE_LOCALE_EN);
    let de = keys(TEMPLATE_LOCALE_DE);
    assert_eq!(en, de, "en.json and de.json must carry the same keys");
    let used = used_keys();
    assert!(!used.is_empty());
    let missing: Vec<&String> = used.difference(&en).collect();
    assert!(
        missing.is_empty(),
        "keys read but never defined: {missing:?}"
    );
    let unused: Vec<&String> = en.difference(&used).collect();
    assert!(unused.is_empty(), "keys defined but never read: {unused:?}");
}

#[test]
fn the_template_practises_the_rules_the_guide_states() {
    for needle in [
        "@app.on_ready",
        "threading.Thread",
        "os.replace",
        "app.data_dir",
        "{**app.settings",
        "app.popups.show",
        "app.set_settings(",
        "usePluginIcon: true",
        "usePluginIcon: false",
        "iconSvg:",
        "icon.png",
        "plugin_guide(section=\"manifest\")",
    ] {
        assert!(
            TEMPLATE_SCRIPT.contains(needle),
            "plugin.py must show {needle}"
        );
    }
    assert!(
        TEMPLATE_VIEWS.contains("data-sb-tween"),
        "one shell behaviour hook"
    );
    assert!(TEMPLATE_VIEWS.contains("sb-field-stack") && TEMPLATE_VIEWS.contains("sb-field__hint"));
}

#[tokio::test]
async fn the_template_section_serves_the_files_with_the_note() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("template".to_string()),
        }))
        .await,
    )
    .expect("template");
    let template = result.template.expect("template served");
    assert!(
        template["note"]
            .as_str()
            .is_some_and(|note| note.contains("smabar.json LAST"))
    );
    let files = template["files"].as_array().expect("files");
    assert_eq!(files.len(), TEMPLATE_FILES.len());
    assert_eq!(
        files.last().map(|f| &f["path"]),
        Some(&Value::from("smabar.json"))
    );
}
