//! Behavior tests for the `ui_kit` coverLayouts section — the nine named
//! tile cover recipes an agent designs bar tiles from.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use super::tests::{test_handler, unwrap_json};
use super::types::UiKitParams;

fn params(section: Option<&str>) -> Parameters<UiKitParams> {
    Parameters(UiKitParams {
        sections: section.map(|section| vec![section.to_string()]),
        classes: None,
    })
}

#[test]
fn cover_layouts_have_the_expected_shape() {
    // An agent picks a layout by id and copies its markup, so every entry
    // must be complete and unambiguous.
    let contract: Value = serde_json::from_str(include_str!("../../../../ui-kit/contract.json"))
        .expect("contract.json must be valid JSON");
    let covers = &contract["coverLayouts"];
    for note in ["what", "settingsConvention", "animationNote"] {
        assert!(covers[note].is_string(), "coverLayouts misses {note}");
    }
    let layouts = covers["layouts"].as_array().expect("coverLayouts.layouts");
    assert_eq!(layouts.len(), 9, "nine named cover layouts");
    let mut ids = std::collections::BTreeSet::new();
    for layout in layouts {
        for field in [
            "id",
            "name",
            "purpose",
            "whenToUse",
            "markup",
            "animation",
            "sizing",
        ] {
            assert!(
                layout[field].is_string(),
                "cover layout misses {field}: {layout}"
            );
        }
        assert!(
            layout["markup"]
                .as_str()
                .unwrap_or_default()
                .contains("sb-"),
            "cover markup without kit classes: {layout}"
        );
        assert!(
            ids.insert(layout["id"].as_str().unwrap_or_default().to_string()),
            "duplicate cover layout id: {layout}"
        );
    }
}

#[tokio::test]
async fn every_reply_carries_the_cover_layouts_index() {
    // A tile designed blind is the most common way a tile ends up ugly:
    // no section may drop the index that says the recipes exist.
    let (_dir, mcp) = test_handler().await;
    for section in [None, Some("classes"), Some("forms"), Some("snippets")] {
        let result = unwrap_json(mcp.ui_kit(params(section)).await).expect("ui_kit");
        let covers = result
            .cover_layouts
            .unwrap_or_else(|| panic!("section {section:?} dropped the coverLayouts index"));
        let layouts = covers["layouts"].as_array().expect("layouts list");
        assert_eq!(layouts.len(), 9, "all nine layouts indexed");
        assert!(
            covers["full"]
                .as_str()
                .unwrap_or_default()
                .contains("coverLayouts"),
            "the index must name the way to the full recipes"
        );
    }
}

#[tokio::test]
async fn the_cover_layouts_section_serves_full_recipes() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.ui_kit(params(Some("coverLayouts"))).await).expect("coverLayouts");
    let covers = result.cover_layouts.expect("coverLayouts section");
    let layouts = covers["layouts"].as_array().expect("layouts list");
    assert_eq!(layouts.len(), 9);
    for layout in layouts {
        assert!(
            layout["markup"].is_string() && layout["sizing"].is_string(),
            "full recipe misses markup or sizing: {layout}"
        );
    }
    assert!(covers["settingsConvention"].is_string());
    assert!(covers["animationNote"].is_string());
}
