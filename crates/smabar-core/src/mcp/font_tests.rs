//! Behavior and schema tests for the MCP font catalog.

use std::fs;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use crate::fonts::FontSource;

use super::tests::{test_handler, unwrap_json};
use super::types::{FontListParams, ThemeWriteParams};

fn params(
    query: Option<&str>,
    source: Option<FontSource>,
    monospaced: Option<bool>,
    limit: Option<usize>,
) -> Parameters<FontListParams> {
    Parameters(FontListParams {
        query: query.map(str::to_string),
        source,
        monospaced,
        limit,
    })
}

#[tokio::test]
async fn google_catalog_ids_and_local_cache_state_are_visible() {
    let (_dir, mcp) = test_handler().await;
    let family_dir = mcp.paths.google_fonts_dir().join("noto-sans");
    fs::create_dir_all(&family_dir).expect("create cached family");
    fs::write(family_dir.join("manifest.json"), "{}").expect("write cache marker");

    let result = unwrap_json(
        mcp.font_list(params(
            Some("noto sans"),
            Some(FontSource::Google),
            None,
            Some(20),
        ))
        .await,
    )
    .expect("list Google fonts");
    assert!(
        result
            .fonts
            .iter()
            .all(|font| font.source == FontSource::Google)
    );
    let noto = result
        .fonts
        .iter()
        .find(|font| font.id == "noto-sans")
        .expect("catalog id is discoverable");
    assert_eq!(noto.family, "Noto Sans");
    assert_eq!(noto.css_stack, "'Noto Sans', system-ui, sans-serif");
    assert!(noto.cached);
}

#[tokio::test]
async fn source_monospace_and_limit_filters_reach_the_core_catalog() {
    let (_dir, mcp) = test_handler().await;

    let system = unwrap_json(
        mcp.font_list(params(
            Some("mono"),
            Some(FontSource::System),
            Some(true),
            None,
        ))
        .await,
    )
    .expect("list system monospace fonts");
    assert!(
        system
            .fonts
            .iter()
            .all(|font| font.source == FontSource::System && font.monospaced)
    );
    assert!(
        system
            .fonts
            .iter()
            .any(|font| font.id == "system:ui-monospace")
    );
    let generic = system
        .fonts
        .iter()
        .find(|font| font.id == "system:ui-monospace")
        .expect("portable generic is listed");
    assert_eq!(generic.css_stack, "ui-monospace, monospace");

    let capped = unwrap_json(
        mcp.font_list(params(
            None,
            Some(FontSource::Google),
            Some(false),
            Some(500),
        ))
        .await,
    )
    .expect("list capped Google fonts");
    assert_eq!(capped.fonts.len(), 100);
    assert!(
        capped
            .fonts
            .iter()
            .all(|font| font.source == FontSource::Google && !font.monospaced)
    );
}

#[test]
fn font_tool_and_theme_write_schemas_explain_the_contract() {
    let tool = super::SmabarMcp::font_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "font_list")
        .expect("font_list is registered");
    let description = tool.description.as_deref().expect("font_list is described");
    for detail in [
        "`cssStack`",
        "google:<catalog-id>",
        "source token `system`",
        "canonicalizes",
        "capped at 100",
    ] {
        assert!(description.contains(detail), "description misses {detail}");
    }
    assert_eq!(
        tool.annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint),
        Some(true)
    );
    for field in ["query", "source", "monospaced", "limit"] {
        assert!(
            tool.input_schema["properties"][field]["description"]
                .as_str()
                .is_some_and(|description| !description.is_empty()),
            "{field} needs an input-schema description"
        );
    }
    assert_eq!(
        tool.input_schema["$defs"]["FontSource"]["enum"],
        json!(["system", "google"])
    );
    let output = tool
        .output_schema
        .as_ref()
        .expect("font_list output schema");
    assert_eq!(output["properties"]["fonts"]["type"], "array");
    let option = &output["$defs"]["FontOption"]["properties"];
    assert_eq!(option["cssStack"]["type"], "string");
    assert!(option["cssStack"]["description"].is_string());

    let theme = schemars::schema_for!(ThemeWriteParams);
    let theme = theme.as_value();
    assert_eq!(theme["properties"]["tokens"]["type"], "object");
    assert_eq!(
        theme["properties"]["tokens"]["additionalProperties"]["type"],
        "string"
    );
    assert_eq!(theme["properties"]["settings"]["type"], "object");
}

/// One spelling everywhere an agent meets the token: the font tool, the
/// theme tool, the result schema and the theme contract.
#[test]
fn the_google_font_token_is_spelled_the_same_everywhere() {
    let syntax = crate::fonts::GOOGLE_FONT_TOKEN_SYNTAX;
    let description =
        |router: rmcp::handler::server::router::tool::ToolRouter<super::SmabarMcp>, name: &str| {
            router
                .list_all()
                .into_iter()
                .find(|tool| tool.name == name)
                .and_then(|tool| tool.description.map(|d| d.to_string()))
                .unwrap_or_else(|| panic!("{name} is described"))
        };
    assert!(description(super::SmabarMcp::font_tool_router(), "font_list").contains(syntax));
    assert!(description(super::SmabarMcp::theme_tool_router(), "theme_write").contains(syntax));
    let schema = serde_json::to_string(&schemars::schema_for!(super::types::FontListResult))
        .expect("schema");
    assert!(
        schema.contains(syntax),
        "FontListResult docs spell the token differently"
    );
    let contract = crate::themes::contract::export();
    assert_eq!(contract["fonts"]["sources"]["google"]["tokenValue"], syntax);
}
