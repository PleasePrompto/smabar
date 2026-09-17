//! The bare `ui_kit()` call is an index, every contract key is a section of
//! its own, and related keys ride along — so an agent never has to guess
//! which neighbour carries the part it needs, and never receives 600 KB of
//! theme internals it did not ask for.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};
use super::types::{UiKitParams, UiKitResult};
use super::ui_kit_tools::{CATEGORIES, RIDERS, SECTIONS};

fn params(sections: Option<&[&str]>) -> Parameters<UiKitParams> {
    Parameters(UiKitParams {
        sections: sections.map(|list| list.iter().map(|s| s.to_string()).collect()),
        classes: None,
    })
}

fn field<'a>(result: &'a UiKitResult, key: &str) -> Option<&'a Value> {
    match key {
        "bestPractices" => result.best_practices.as_ref(),
        "coverLayouts" => result.cover_layouts.as_ref(),
        "snippets" => result.snippets.as_ref(),
        "behaviour" => result.behaviour.as_ref(),
        "conventions" => result.conventions.as_ref(),
        "sanitizer" => result.sanitizer.as_ref(),
        "media" => result.media.as_ref(),
        "formContract" => result.form_contract.as_ref(),
        "branding" => result.branding.as_ref(),
        "renderTargets" => result.render_targets.as_ref(),
        "tileChrome" => result.tile_chrome.as_ref(),
        "charts" => result.charts.as_ref(),
        "icons" => result.icons.as_ref(),
        "classes" => result.classes.as_ref(),
        other => panic!("no accessor for section {other}"),
    }
}

#[tokio::test]
async fn the_bare_call_answers_with_a_small_index() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.ui_kit(params(None)).await).expect("index");
    let index = result
        .sections
        .as_ref()
        .expect("the index names the sections");
    assert!(index["sections"]["coverLayouts"].is_string());
    assert!(
        index["snippets"]
            .as_array()
            .is_some_and(|names| !names.is_empty())
    );
    assert!(result.design_rules.is_some() && result.tile_sizing.is_some());
    assert!(result.class_categories.is_some());
    assert!(result.behaviour.is_some() && result.cover_layouts.is_some());
    assert!(result.tokens.is_none() && result.token_contract.is_none());
    assert!(result.classes.is_none() && result.snippets.is_none());
    let size = serde_json::to_string(&result).expect("serialize").len();
    assert!(
        size < 16_000,
        "the index must stay readable, got {size} bytes"
    );
}

#[tokio::test]
async fn plugin_authors_discover_flyout_width_and_both_scroll_layouts_through_mcp() {
    let (_dir, mcp) = test_handler().await;
    let guide = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams { section: None }))
            .await,
    )
    .expect("plugin guide start");
    let style = guide.style.expect("start carries style guidance");
    let flyout = style["flyout"].as_str().expect("flyout guidance");
    for term in [
        "data-sb-flyout-width",
        "conventions",
        "layout",
        "snippets",
        "sb-split",
    ] {
        assert!(flyout.contains(term), "plugin guide omits {term}");
    }
    let index = unwrap_json(mcp.ui_kit(params(None)).await).expect("kit index");
    let sections = index.sections.expect("section index");
    assert!(
        sections["sections"]["conventions"]
            .as_str()
            .expect("conventions")
            .contains("width")
    );
    let kit = unwrap_json(
        mcp.ui_kit(params(Some(&["conventions", "layout", "snippets"])))
            .await,
    )
    .expect("documented follow-up call");
    let conventions = kit.conventions.expect("conventions");
    assert_eq!(
        conventions["flyoutWidth"]["attribute"],
        "data-sb-flyout-width"
    );
    let classes = kit.classes.expect("layout classes");
    assert!(
        classes
            .as_array()
            .expect("class entries")
            .iter()
            .any(|class| class["name"] == "sb-split")
    );
    let snippets = kit.snippets.expect("copyable snippets");
    for name in ["wideFlyout", "screenFlyout", "splitFlyout"] {
        assert!(flyout.contains(name), "guide does not point to {name}");
        assert!(
            sections["snippets"]
                .as_array()
                .expect("snippet index")
                .iter()
                .any(|entry| entry == name)
        );
        assert!(
            snippets[name]
                .as_str()
                .expect("HTML example")
                .contains("data-sb-flyout-width")
        );
    }
}

#[tokio::test]
async fn every_contract_key_is_addressable_as_its_own_section() {
    let (_dir, mcp) = test_handler().await;
    for section in SECTIONS
        .iter()
        .filter(|section| !matches!(**section, "all" | "tokens"))
    {
        let result = unwrap_json(mcp.ui_kit(params(Some(&[section]))).await)
            .unwrap_or_else(|error| panic!("section {section}: {}", error.message));
        assert!(
            field(&result, section).is_some(),
            "section {section} answers with itself"
        );
        assert!(
            result.tokens.is_none(),
            "section {section} must not drag the tokens along"
        );
        assert!(
            result.sections.is_none(),
            "a named section is not the index"
        );
    }
}

#[tokio::test]
async fn riders_travel_with_their_hosts() {
    let (_dir, mcp) = test_handler().await;
    for (rider, hosts) in RIDERS {
        for host in *hosts {
            let result = unwrap_json(mcp.ui_kit(params(Some(&[host]))).await)
                .unwrap_or_else(|error| panic!("host {host}: {}", error.message));
            assert!(
                field(&result, rider).is_some(),
                "{host} must carry {rider} along"
            );
        }
    }
    // And a category that has no rider stays lean.
    let layout = unwrap_json(mcp.ui_kit(params(Some(&["layout"]))).await).expect("layout");
    assert!(layout.charts.is_none() && layout.media.is_none() && layout.form_contract.is_none());
}

#[test]
fn the_section_index_describes_every_section_and_category() {
    let contract: Value =
        serde_json::from_str(include_str!("../../../../ui-kit/contract.json")).expect("contract");
    let index = contract["sections"].as_object().expect("sections index");
    for name in SECTIONS.iter().chain(CATEGORIES) {
        assert!(
            index.get(*name).is_some_and(Value::is_string),
            "contract.sections misses {name}"
        );
    }
    for name in index.keys() {
        assert!(
            SECTIONS.contains(&name.as_str()) || CATEGORIES.contains(&name.as_str()),
            "contract.sections names {name}, which the tool does not serve"
        );
    }
}

#[test]
fn the_tool_description_names_every_section() {
    let tool = super::SmabarMcp::ui_kit_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "ui_kit")
        .expect("ui_kit tool");
    let description = tool.description.as_deref().unwrap_or_default();
    for name in SECTIONS.iter().chain(CATEGORIES) {
        assert!(description.contains(name), "description misses {name}");
    }
    assert!(description.contains("INDEX"));
}

/// Only the built-in command values do anything; the contract's prose has
/// to name every one it allows, or an agent hunts for the rest.
#[test]
fn the_sanitizer_prose_names_every_command_value() {
    let contract: Value =
        serde_json::from_str(include_str!("../../../../ui-kit/contract.json")).expect("contract");
    let prose = serde_json::to_string(&contract["sanitizer"]["nativeInteraction"]).expect("prose");
    for value in contract["sanitizer"]["commandValues"]
        .as_array()
        .expect("commandValues")
    {
        let value = value.as_str().unwrap_or_default();
        assert!(
            prose.contains(value),
            "sanitizer.nativeInteraction never names {value}"
        );
    }
}
