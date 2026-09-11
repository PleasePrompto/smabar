//! The bare `plugin_guide()` call is the START document: compact, complete
//! about the steps, and every pointer in it resolves.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use super::guide_tools::GUIDE;
use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};
use super::types::UiKitParams;

fn params(section: Option<&str>) -> Parameters<GuideParams> {
    Parameters(GuideParams {
        section: section.map(str::to_string),
    })
}

#[tokio::test]
async fn the_bare_call_is_a_compact_start_document() {
    let (_dir, mcp) = test_handler().await;
    let start = unwrap_json(mcp.plugin_guide(params(None)).await).expect("start");
    let size = serde_json::to_string(&start).expect("serialize").len();
    assert!(
        size < 16_000,
        "the start document must stay readable, got {size} bytes"
    );
    let steps = start
        .golden_path
        .as_ref()
        .and_then(Value::as_array)
        .expect("steps");
    assert_eq!(steps.len(), 6);
    let named = unwrap_json(mcp.plugin_guide(params(Some("start"))).await).expect("start by name");
    assert_eq!(
        serde_json::to_string(&named).expect("serialize"),
        serde_json::to_string(&start).expect("serialize"),
        "start is also reachable by name"
    );
}

#[test]
fn the_section_index_describes_every_section() {
    let index = GUIDE["sections"].as_object().expect("sections index");
    for name in [
        "start",
        "capabilities",
        "manifest",
        "sdk",
        "storage",
        "lifecycle",
        "debugging",
        "template",
    ] {
        assert!(
            index.get(name).is_some_and(Value::is_string),
            "sections misses {name}"
        );
    }
}

#[test]
fn the_terms_are_the_words_the_replies_use() {
    let terms = GUIDE["terms"].as_object().expect("terms");
    for term in [
        "golden path",
        "surface",
        "sibling",
        "clean start",
        "photograph",
        "cover",
    ] {
        assert!(
            terms.get(term).is_some_and(Value::is_string),
            "terms misses {term}"
        );
    }
}

/// Every `read` pointer in the golden path names a real tool, and the
/// guide/kit pointers execute — a step that points at nothing is a dead end.
#[tokio::test]
async fn every_golden_path_pointer_resolves() {
    let (_dir, mcp) = test_handler().await;
    let tools: Vec<String> = mcp
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    for step in GUIDE["goldenPath"].as_array().expect("steps") {
        for read in step["read"].as_array().expect("read list") {
            let tool = read["tool"].as_str().expect("tool name");
            assert!(
                tools.contains(&tool.to_string()),
                "step {} points at {tool}",
                step["step"]
            );
            match tool {
                "plugin_guide" => {
                    let params: GuideParams =
                        serde_json::from_value(read["arguments"].clone()).expect("guide params");
                    unwrap_json(mcp.plugin_guide(Parameters(params)).await)
                        .unwrap_or_else(|error| panic!("step {}: {}", step["step"], error.message));
                }
                "ui_kit" => {
                    let params: UiKitParams =
                        serde_json::from_value(read["arguments"].clone()).expect("kit params");
                    unwrap_json(mcp.ui_kit(Parameters(params)).await)
                        .unwrap_or_else(|error| panic!("step {}: {}", step["step"], error.message));
                }
                _ => {}
            }
        }
    }
}
