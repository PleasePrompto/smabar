//! Multi-section and design-guidance tests for the `ui_kit` MCP tool.

use std::collections::HashSet;

use rmcp::handler::server::wrapper::Parameters;
use schemars::schema_for;
use serde_json::Value;

use super::tests::{test_handler, unwrap_json};
use super::types::UiKitParams;

fn params(sections: &[&str]) -> Parameters<UiKitParams> {
    Parameters(UiKitParams {
        sections: Some(
            sections
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
        ),
        classes: None,
    })
}

#[test]
fn best_practices_cover_the_complete_design_review() {
    let contract: Value = serde_json::from_str(include_str!("../../../../ui-kit/contract.json"))
        .expect("contract.json must be valid JSON");
    let practices = &contract["bestPractices"];
    for section in [
        "workflow",
        "hierarchy",
        "states",
        "accessibility",
        "forms",
        "themeAndMotion",
        "media",
        "verification",
    ] {
        assert!(
            practices[section]
                .as_array()
                .is_some_and(|entries| !entries.is_empty()),
            "bestPractices misses {section}"
        );
    }
    let serialized = practices.to_string();
    for needle in [
        "coverLayouts",
        "follow-up",
        "hasFlyout",
        "sb-asset:",
        "plugin_logs",
        "bar_screenshot",
        "open_flyout",
        "placeholder",
        "sb-field-stack",
        "INSPECT",
    ] {
        assert!(serialized.contains(needle), "bestPractices misses {needle}");
    }
}

#[tokio::test]
async fn multi_section_lookup_returns_the_requested_design_slices() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(
        mcp.ui_kit(params(&[
            "coverLayouts",
            "bestPractices",
            "data",
            "snippets",
        ]))
        .await,
    )
    .expect("multi-section lookup");

    assert!(result.best_practices.is_some());
    assert!(result.snippets.is_some());
    assert!(
        result
            .cover_layouts
            .as_ref()
            .is_some_and(|covers| covers["settingsConvention"].is_string()),
        "coverLayouts must carry the full recipes"
    );
    let classes = result
        .classes
        .as_ref()
        .and_then(Value::as_array)
        .expect("data classes");
    assert!(!classes.is_empty());
    assert!(classes.iter().all(|entry| entry["category"] == "data"));
    assert!(result.tokens.is_none() && result.sanitizer.is_none());
}

#[tokio::test]
async fn category_sections_form_a_deduplicated_union() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.ui_kit(params(&["forms", "feedback", "forms"])).await)
        .expect("category union");
    let classes = result
        .classes
        .as_ref()
        .and_then(Value::as_array)
        .expect("classes");
    let mut names = HashSet::new();
    for entry in classes {
        assert!(matches!(
            entry["category"].as_str(),
            Some("forms" | "feedback")
        ));
        assert!(
            names.insert(entry["name"].as_str().expect("class name")),
            "duplicate class returned"
        );
    }
    assert!(result.form_contract.is_some());
}

#[tokio::test]
async fn multi_section_lookup_rejects_ambiguous_or_unbounded_requests() {
    let (_dir, mcp) = test_handler().await;
    for sections in [
        Vec::new(),
        vec!["tokens", "icons", "sanitizer", "conventions", "snippets"],
        vec!["all", "data"],
        vec!["coverLayouts", "unknown"],
    ] {
        let error = unwrap_json(mcp.ui_kit(params(&sections)).await).expect_err("invalid sections");
        assert!(!error.message.is_empty());
    }

    let error = unwrap_json(
        mcp.ui_kit(Parameters(UiKitParams {
            sections: Some(vec!["data".to_string()]),
            classes: Some(vec!["sb-section".to_string()]),
        }))
        .await,
    )
    .expect_err("section list plus exact classes");
    assert!(error.message.contains("alternative"));
}

#[test]
fn sections_schema_and_tool_description_expose_the_bounded_lookup() {
    let schema = serde_json::to_value(schema_for!(UiKitParams)).expect("serialize schema");
    let sections = &schema["properties"]["sections"];
    let serialized = sections.to_string();
    assert!(serialized.contains("\"minItems\":1"), "{sections}");
    assert!(serialized.contains("\"maxItems\":4"), "{sections}");
    assert!(schema["properties"].get("section").is_none());
    assert!(
        serde_json::from_value::<UiKitParams>(serde_json::json!({"section": "forms"})).is_err()
    );

    let described = super::SmabarMcp::ui_kit_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "ui_kit")
        .and_then(|tool| tool.description)
        .expect("ui_kit is described")
        .to_string();
    assert!(described.contains("sections"), "{described}");
    assert!(described.contains("bestPractices"), "{described}");
}
