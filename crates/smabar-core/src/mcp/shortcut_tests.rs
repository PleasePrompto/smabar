//! Behavior tests for the shortcut MCP tools against .desktop fixtures.

use std::fs;
use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;

use crate::config::SpecialShortcut;

use super::SmabarMcp;
use super::tests::{test_handler, unwrap_json};
use super::types::{AppSearchParams, ShortcutAddParams, ShortcutIdParams};

fn write_desktop(base: &Path, file: &str, content: &str) {
    let dir = base.join("applications");
    fs::create_dir_all(&dir).expect("create applications dir");
    fs::write(dir.join(file), content).expect("write .desktop fixture");
}

/// A handler with two launchable fixture apps installed.
async fn handler_with_apps() -> (tempfile::TempDir, SmabarMcp) {
    let (dir, mcp) = test_handler().await;
    let base = mcp.paths.base_dir().to_path_buf();
    write_desktop(
        &base,
        "calc.desktop",
        "[Desktop Entry]\nType=Application\nName=Calculator\nComment=Do math\nExec=cargo --version %u\n",
    );
    write_desktop(
        &base,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=cargo --version\n",
    );
    mcp.shortcuts.refresh();
    (dir, mcp)
}

fn search_params(query: &str) -> Parameters<AppSearchParams> {
    Parameters(AppSearchParams {
        query: query.to_string(),
    })
}

fn add_params(desktop_id: Option<&str>, index: Option<usize>) -> Parameters<ShortcutAddParams> {
    Parameters(ShortcutAddParams {
        desktop_id: desktop_id.map(str::to_string),
        path: None,
        url: None,
        special: None,
        label: None,
        index,
        separator: false,
    })
}

#[tokio::test]
async fn shortcut_add_supports_multiple_separators_and_rejects_launch() {
    let (_dir, mcp) = handler_with_apps().await;
    let separator = || {
        Parameters(ShortcutAddParams {
            desktop_id: None,
            path: None,
            url: None,
            special: None,
            label: None,
            index: None,
            separator: true,
        })
    };

    unwrap_json(mcp.shortcut_add(separator()).await).expect("first separator");
    unwrap_json(mcp.shortcut_add(separator()).await).expect("second separator");
    let list = unwrap_json(mcp.shortcut_list().await).expect("list separators");
    assert_eq!(list.pinned.len(), 2);
    assert!(list.pinned.iter().all(|entry| entry.separator));
    assert_ne!(list.pinned[0].id, list.pinned[1].id);

    let err = unwrap_json(
        mcp.shortcut_launch(Parameters(ShortcutIdParams {
            id: list.pinned[0].id.clone(),
        }))
        .await,
    )
    .expect_err("separator launch");
    assert!(
        err.message.contains("cannot be launched"),
        "{}",
        err.message
    );

    let err = unwrap_json(
        mcp.shortcut_add(Parameters(ShortcutAddParams {
            desktop_id: Some("calc.desktop".to_string()),
            path: None,
            url: None,
            special: None,
            label: None,
            index: None,
            separator: true,
        }))
        .await,
    )
    .expect_err("separator with source");
    assert!(err.message.contains("separator=true"), "{}", err.message);
}

#[test]
fn shortcut_add_separator_is_optional_and_defaults_false() {
    let params: ShortcutAddParams =
        serde_json::from_str(r#"{"desktopId":"calc.desktop"}"#).expect("parse params");
    assert!(!params.separator);

    let schema = schemars::schema_for!(ShortcutAddParams);
    let required = schema
        .as_value()
        .get("required")
        .and_then(serde_json::Value::as_array);
    assert!(required.is_none_or(|fields| !fields.iter().any(|field| field == "separator")));

    let special: ShortcutAddParams =
        serde_json::from_str(r#"{"special":"computer"}"#).expect("parse special source");
    assert_eq!(special.special, Some(SpecialShortcut::Computer));
    assert!(serde_json::from_str::<ShortcutAddParams>(r#"{"special":"downloads"}"#).is_err());
}

#[tokio::test]
async fn app_search_lists_and_filters_without_icon_data() {
    let (_dir, mcp) = handler_with_apps().await;

    let all = unwrap_json(mcp.app_search(search_params("")).await).expect("list all");
    let names: Vec<&str> = all.apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Calculator", "Editor"]);

    let math = unwrap_json(mcp.app_search(search_params("math")).await).expect("by comment");
    assert_eq!(math.apps.len(), 1);
    assert_eq!(math.apps[0].desktop_id.as_deref(), Some("calc.desktop"));
    assert!(math.apps[0].path.is_none());
    assert_eq!(math.apps[0].comment.as_deref(), Some("Do math"));
}

#[tokio::test]
async fn shortcut_add_lists_file_folder_and_special_sources() {
    let (dir, mcp) = handler_with_apps().await;
    let folder = dir.path().join("Pinned Folder");
    fs::create_dir(&folder).expect("create folder");
    let file = dir.path().join("notes.txt");
    fs::write(&file, b"notes").expect("create file");

    unwrap_json(
        mcp.shortcut_add(Parameters(ShortcutAddParams {
            desktop_id: None,
            path: Some(folder.to_string_lossy().into_owned()),
            url: None,
            special: None,
            label: None,
            index: None,
            separator: false,
        }))
        .await,
    )
    .expect("pin folder");
    unwrap_json(
        mcp.shortcut_add(Parameters(ShortcutAddParams {
            desktop_id: None,
            path: Some(file.to_string_lossy().into_owned()),
            url: None,
            special: None,
            label: None,
            index: None,
            separator: false,
        }))
        .await,
    )
    .expect("pin file");
    unwrap_json(
        mcp.shortcut_add(Parameters(ShortcutAddParams {
            desktop_id: None,
            path: None,
            url: None,
            special: Some(SpecialShortcut::Trash),
            label: None,
            index: None,
            separator: false,
        }))
        .await,
    )
    .expect("pin special item");

    let list = unwrap_json(mcp.shortcut_list().await).expect("list sources");
    assert_eq!(list.pinned[0].path.as_deref(), Some(folder.as_path()));
    assert_eq!(list.pinned[0].special, None);
    assert_eq!(list.pinned[1].path.as_deref(), Some(file.as_path()));
    assert_eq!(list.pinned[1].special, None);
    assert_eq!(list.pinned[2].path, None);
    assert_eq!(list.pinned[2].special, Some(SpecialShortcut::Trash));
}

#[tokio::test]
async fn shortcut_add_list_launch_remove_roundtrip() {
    let (_dir, mcp) = handler_with_apps().await;
    let mut changes = mcp.config.subscribe();

    let ack = unwrap_json(
        mcp.shortcut_add(add_params(Some("calc.desktop"), None))
            .await,
    )
    .expect("pin calc");
    assert!(ack.message.contains("sc-"), "ack: {}", ack.message);
    let change = changes.try_recv().expect("pin must broadcast");
    assert!(change.shortcuts_changed());

    // Insert the editor in front via index 0.
    unwrap_json(
        mcp.shortcut_add(add_params(Some("editor.desktop"), Some(0)))
            .await,
    )
    .expect("pin editor");

    let list = unwrap_json(mcp.shortcut_list().await).expect("list");
    let labels: Vec<&str> = list.pinned.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, vec!["Editor", "Calculator"]);
    assert!(list.pinned.iter().all(|p| !p.icon_resolved));

    let calc_id = list.pinned[1].id.clone();
    let ack = unwrap_json(
        mcp.shortcut_launch(Parameters(ShortcutIdParams {
            id: calc_id.clone(),
        }))
        .await,
    )
    .expect("launch");
    assert!(ack.message.contains("launched"));

    unwrap_json(
        mcp.shortcut_remove(Parameters(ShortcutIdParams {
            id: calc_id.clone(),
        }))
        .await,
    )
    .expect("unpin");
    let list = unwrap_json(mcp.shortcut_list().await).expect("list again");
    assert_eq!(list.pinned.len(), 1);
    assert_eq!(mcp.config.current().shortcuts.pinned.len(), 1);
}

#[tokio::test]
async fn shortcut_tools_reject_unknown_sources_and_ids() {
    let (_dir, mcp) = handler_with_apps().await;

    let err = unwrap_json(
        mcp.shortcut_add(add_params(Some("ghost.desktop"), None))
            .await,
    )
    .expect_err("unknown desktop id");
    assert!(err.message.contains("ghost.desktop"), "{}", err.message);

    let err =
        unwrap_json(mcp.shortcut_add(add_params(None, None)).await).expect_err("no source at all");
    assert!(err.message.contains("desktopId"), "{}", err.message);

    // Pinning the same app twice is rejected and changes nothing.
    unwrap_json(
        mcp.shortcut_add(add_params(Some("calc.desktop"), None))
            .await,
    )
    .expect("pin");
    let err = unwrap_json(
        mcp.shortcut_add(add_params(Some("calc.desktop"), None))
            .await,
    )
    .expect_err("duplicate pin");
    assert!(err.message.contains("already pinned"), "{}", err.message);
    assert_eq!(mcp.config.current().shortcuts.pinned.len(), 1);

    for tool_err in [
        unwrap_json(
            mcp.shortcut_remove(Parameters(ShortcutIdParams {
                id: "sc-missing0".to_string(),
            }))
            .await,
        )
        .expect_err("unknown unpin id"),
        unwrap_json(
            mcp.shortcut_launch(Parameters(ShortcutIdParams {
                id: "sc-missing0".to_string(),
            }))
            .await,
        )
        .expect_err("unknown launch id"),
    ] {
        assert!(
            tool_err.message.contains("sc-missing0"),
            "{}",
            tool_err.message
        );
    }
}
