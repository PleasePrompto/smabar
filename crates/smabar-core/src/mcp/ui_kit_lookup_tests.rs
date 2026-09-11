//! Focused class-name lookup tests for the `ui_kit` MCP tool.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use super::tests::{test_handler, unwrap_json};
use super::types::{UiKitParams, UiKitResult};

fn params(section: Option<&str>, classes: Option<&[&str]>) -> Parameters<UiKitParams> {
    Parameters(UiKitParams {
        sections: section.map(|section| vec![section.to_string()]),
        classes: classes.map(|names| names.iter().map(|name| (*name).to_string()).collect()),
    })
}

fn class_entries(result: &UiKitResult) -> &[Value] {
    result
        .classes
        .as_ref()
        .and_then(Value::as_array)
        .expect("classes")
}

#[tokio::test]
async fn exact_class_lookup_returns_only_the_requested_names_in_order() {
    let (_dir, mcp) = test_handler().await;
    let requested = ["sb-table", "sb-section", "sb-btn--primary"];
    let result =
        unwrap_json(mcp.ui_kit(params(None, Some(&requested))).await).expect("exact class lookup");
    let names: Vec<&str> = class_entries(&result)
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert_eq!(names, requested);
    assert!(result.class_categories.is_some());
    assert!(result.tokens.is_none() && result.icons.is_none() && result.snippets.is_none());
}

#[tokio::test]
async fn exact_class_lookup_rejects_unknown_names_and_a_section() {
    let (_dir, mcp) = test_handler().await;
    let error =
        unwrap_json(mcp.ui_kit(params(None, Some(&["sb-crad"]))).await).expect_err("unknown class");
    assert!(error.message.contains("sb-crad"));
    assert!(error.message.contains("exact"));

    let error = unwrap_json(
        mcp.ui_kit(params(Some("content"), Some(&["sb-section"])))
            .await,
    )
    .expect_err("ambiguous lookup");
    assert!(error.message.contains("alternative"));
}

#[tokio::test]
async fn sb_section_is_discoverable_in_all_and_in_content() {
    let (_dir, mcp) = test_handler().await;
    for section in [Some("all"), Some("content")] {
        let result = unwrap_json(mcp.ui_kit(params(section, None)).await).expect("ui_kit");
        let entry = class_entries(&result)
            .iter()
            .find(|entry| entry["name"] == "sb-section")
            .unwrap_or_else(|| panic!("sb-section missing from {section:?}"));
        assert_eq!(entry["category"], "content");
    }
}
