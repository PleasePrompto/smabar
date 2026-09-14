//! Bounded theme discovery and removal through the public MCP handlers.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use super::tests::{test_handler, unwrap_json};
use super::types::{ThemeGetParams, ThemeRemoveParams};

#[tokio::test]
async fn theme_queries_stay_small_and_preserve_access_to_the_full_contract() {
    let (_dir, mcp) = test_handler().await;
    let full = crate::themes::contract::export();
    for name in [None, Some("default"), Some("paper")] {
        let result = unwrap_json(
            mcp.theme_get(Parameters(ThemeGetParams {
                name: name.map(str::to_string),
                contract_path: None,
                offset: None,
            }))
            .await,
        )
        .expect("compact theme");
        assert!(serde_json::to_vec(&result).expect("JSON").len() < 24 * 1024);
        assert_eq!(result.contract["fonts"], full["fonts"]);
        assert_eq!(result.contract["settings"], full["settings"]);
    }
    for path in [
        "",
        "/baseTokens",
        "/publicComponentTokens",
        "/themeSchema",
        "/fonts",
        "/referenceThemes/paper",
        "/baseTokens/--sb-accent/type",
    ] {
        let result = unwrap_json(
            mcp.theme_get(Parameters(ThemeGetParams {
                name: None,
                contract_path: Some(path.to_string()),
                offset: None,
            }))
            .await,
        )
        .expect("contract query");
        assert!(
            serde_json::to_vec(&result).expect("JSON").len() < 24 * 1024,
            "{path}"
        );
        if result.contract["kind"] == "index" {
            for entry in result.contract["entries"].as_array().expect("index") {
                assert!(
                    full.pointer(entry["path"].as_str().expect("path"))
                        .is_some()
                );
            }
        } else {
            assert_eq!(
                &result.contract,
                full.pointer(path).expect("original fragment")
            );
        }
    }
    let mut paths = Vec::new();
    let mut offset = Some(0);
    while let Some(start) = offset {
        let result = unwrap_json(
            mcp.theme_get(Parameters(ThemeGetParams {
                name: None,
                contract_path: Some("/baseTokens".to_string()),
                offset: Some(start),
            }))
            .await,
        )
        .expect("index page");
        paths.extend(
            result.contract["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .cloned(),
        );
        offset = result.contract["nextOffset"]
            .as_u64()
            .map(|value| usize::try_from(value).expect("offset"));
    }
    assert_eq!(
        paths.len(),
        full["baseTokens"].as_object().expect("base tokens").len()
    );
    paths.sort_by_key(|entry| entry["path"].as_str().expect("path").to_string());
    paths.dedup();
    assert_eq!(
        paths.len(),
        full["baseTokens"].as_object().expect("base tokens").len()
    );
    for (path, offset) in [
        (Some("no-pointer"), 0),
        (Some("/missing"), 0),
        (None, 1),
        (Some("/fonts"), 1),
        (Some("/baseTokens"), usize::MAX),
    ] {
        unwrap_json(
            mcp.theme_get(Parameters(ThemeGetParams {
                name: None,
                contract_path: path.map(str::to_string),
                offset: Some(offset),
            }))
            .await,
        )
        .expect_err("invalid contract query");
    }
}

#[tokio::test]
async fn theme_remove_preserves_active_and_bundled_themes_and_deletes_dropins() {
    let (_dir, mcp) = test_handler().await;
    unwrap_json(
        mcp.theme_write(Parameters(
            serde_json::from_value(json!({
                "name": "removable", "tokens": {"--sb-accent": "#123456"}
            }))
            .expect("write params"),
        ))
        .await,
    )
    .expect("write theme");
    unwrap_json(
        mcp.settings_set(Parameters(
            serde_json::from_value(json!({
                "path": "theme", "value": "removable"
            }))
            .expect("activation params"),
        ))
        .await,
    )
    .expect("activate theme");
    let active = mcp.config.current();
    let file = mcp.paths.themes_dir().join("removable.json");
    let bytes = std::fs::read(&file).expect("theme");
    for name in ["removable", "default", "paper", "ghost", "../outside"] {
        unwrap_json(
            mcp.theme_remove(Parameters(ThemeRemoveParams {
                name: name.to_string(),
            }))
            .await,
        )
        .expect_err("active, bundled, missing or invalid theme");
        assert_eq!(mcp.config.current(), active);
        assert_eq!(std::fs::read(&file).expect("theme preserved"), bytes);
    }
    unwrap_json(
        mcp.settings_set(Parameters(
            serde_json::from_value(json!({
                "path": "theme", "value": "default"
            }))
            .expect("activation params"),
        ))
        .await,
    )
    .expect("switch away");
    unwrap_json(
        mcp.theme_remove(Parameters(ThemeRemoveParams {
            name: "removable".to_string(),
        }))
        .await,
    )
    .expect("remove");
    assert!(!file.exists());
    let listed = unwrap_json(mcp.theme_list().await).expect("list");
    assert!(listed.themes.iter().all(|theme| theme.name != "removable"));
}
