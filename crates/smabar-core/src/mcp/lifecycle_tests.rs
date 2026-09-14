//! MCP tests for the three off-switches: hide a tile, deactivate a plugin,
//! delete a plugin. What matters here is that they stay THREE things — the
//! tools must not quietly do each other's job.

use std::fs;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use super::plugin_types::PluginIdParams;
use super::tests::{test_handler, unwrap_json};
use super::types::{PluginSetActiveParams, PluginSetVisibleParams};

const MANIFEST: &str = r#"{"id":"hello","name":"Hello","version":"1","protocolVersion":1,
"runtime":"exec","command":["true"],"tiles":[{"id":"greeting","name":"Greeting"}]}"#;

fn install(mcp: &super::SmabarMcp, id: &str) {
    let dir = mcp.paths.plugins_dir().join(id);
    fs::create_dir_all(&dir).expect("create plugin dir");
    fs::write(dir.join("smabar.json"), MANIFEST.replace("hello", id)).expect("write manifest");
}

fn set_active(id: &str, active: bool) -> Parameters<PluginSetActiveParams> {
    Parameters(PluginSetActiveParams {
        id: id.to_string(),
        active,
    })
}

#[tokio::test]
async fn plugin_lists_serialize_required_fields_with_and_without_hidden_tiles() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");
    mcp.supervisor
        .restart("hello")
        .await
        .expect("register plugin");

    for hidden in [false, true] {
        mcp.config
            .update(|current| {
                let mut updated = current.clone();
                updated.plugins_hidden = vec!["plugin:other:greeting".to_string()];
                if hidden {
                    updated
                        .plugins_hidden
                        .push("plugin:hello:greeting".to_string());
                }
                (updated, ())
            })
            .expect("set hidden tiles");
        for (name, value) in [
            (
                "plugin_list",
                serde_json::to_value(unwrap_json(mcp.plugin_list().await).expect("list")),
            ),
            (
                "bar_get_state",
                serde_json::to_value(unwrap_json(mcp.bar_get_state().await).expect("state")),
            ),
        ] {
            let value = value.expect("serialize response");
            let tools = mcp.tool_router.list_all();
            let schema = tools
                .iter()
                .find(|tool| tool.name == name)
                .and_then(|tool| tool.output_schema.as_ref())
                .expect("advertised output schema");
            let required = schema["$defs"]["PluginInfoOut"]["required"]
                .as_array()
                .expect("required fields");
            assert!(required.contains(&json!("hiddenTiles")));
            let plugins = value["plugins"].as_array().expect("plugins");
            assert_eq!(plugins.len(), 1);
            for field in required {
                let field = field.as_str().expect("field name");
                assert!(
                    plugins[0].get(field).is_some(),
                    "{name}: missing required property {field}"
                );
            }
            assert_eq!(
                plugins[0]["hiddenTiles"],
                if hidden {
                    json!(["greeting"])
                } else {
                    json!([])
                }
            );
            assert_eq!(plugins[0]["updateAvailable"], false);
        }
    }
    mcp.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn reloading_a_deactivated_plugin_points_to_the_activation_tool() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");
    unwrap_json(mcp.plugin_set_active(set_active("hello", false)).await).expect("deactivate");
    let error = unwrap_json(
        mcp.plugin_reload(Parameters(PluginIdParams {
            id: "hello".to_string(),
        }))
        .await,
    )
    .expect_err("deactivated");
    assert!(
        error.message.contains("plugin_set_active"),
        "{}",
        error.message
    );
    assert!(error.message.contains("hello") && error.message.contains("true"));
}

#[tokio::test]
async fn plugin_set_active_writes_the_deactivation_list_and_is_idempotent() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");

    let off =
        unwrap_json(mcp.plugin_set_active(set_active("hello", false)).await).expect("deactivate");
    assert!(off.message.contains("deactivated"), "{}", off.message);
    assert_eq!(mcp.config.current().plugins_deactivated, vec!["hello"]);

    let again = unwrap_json(mcp.plugin_set_active(set_active("hello", false)).await)
        .expect("deactivate again");
    assert!(again.message.contains("already"), "{}", again.message);
    assert_eq!(mcp.config.current().plugins_deactivated, vec!["hello"]);

    let on = unwrap_json(mcp.plugin_set_active(set_active("hello", true)).await).expect("activate");
    assert!(on.message.contains("active"), "{}", on.message);
    assert!(mcp.config.current().plugins_deactivated.is_empty());
}

/// Writing a deactivation entry for an id nothing is installed under would
/// silently kill a plugin of that name later.
#[tokio::test]
async fn plugin_set_active_refuses_ids_that_are_not_installed() {
    let (_dir, mcp) = test_handler().await;
    let error = unwrap_json(mcp.plugin_set_active(set_active("ghost", false)).await)
        .expect_err("nothing installed");
    assert!(error.message.contains("plugin_list"), "{}", error.message);
    assert!(mcp.config.current().plugins_deactivated.is_empty());

    let error = unwrap_json(mcp.plugin_set_active(set_active("Bad Id", false)).await)
        .expect_err("invalid id");
    assert!(error.message.contains("[a-z0-9-]"), "{}", error.message);
}

#[tokio::test]
async fn plugin_set_visible_hides_one_tile_without_deactivating_its_plugin() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");

    let hidden = unwrap_json(
        mcp.plugin_set_visible(Parameters(PluginSetVisibleParams {
            tile_id: "plugin:hello:greeting".to_string(),
            visible: false,
        }))
        .await,
    )
    .expect("hide");
    // The answer has to name the difference, or an agent will reach for this
    // when it meant to stop the plugin.
    assert!(
        hidden.message.contains("keeps running"),
        "{}",
        hidden.message
    );
    assert_eq!(
        mcp.config.current().plugins_hidden,
        vec!["plugin:hello:greeting"]
    );
    assert!(
        mcp.config.current().plugins_deactivated.is_empty(),
        "hiding a tile must never deactivate its plugin"
    );

    let shown = unwrap_json(
        mcp.plugin_set_visible(Parameters(PluginSetVisibleParams {
            tile_id: "plugin:hello:greeting".to_string(),
            visible: true,
        }))
        .await,
    )
    .expect("show");
    assert!(shown.message.contains("shown"), "{}", shown.message);
    assert!(mcp.config.current().plugins_hidden.is_empty());
}

#[tokio::test]
async fn plugin_remove_deletes_code_data_and_log_and_says_it_cannot_be_undone() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");
    fs::create_dir_all(mcp.paths.plugin_data_dir("hello")).expect("create data dir");
    fs::write(mcp.paths.plugin_data_dir("hello").join("x.db"), "rows").expect("write data");
    fs::create_dir_all(mcp.paths.logs_dir()).expect("create logs dir");
    fs::write(mcp.paths.logs_dir().join("plugin-hello.log"), "{}\n").expect("write log");

    let removed = unwrap_json(
        mcp.plugin_remove(Parameters(PluginIdParams {
            id: "hello".to_string(),
        }))
        .await,
    )
    .expect("remove");
    assert!(
        removed.message.contains("permanently"),
        "{}",
        removed.message
    );

    assert!(!mcp.paths.plugins_dir().join("hello").exists());
    assert!(!mcp.paths.plugin_data_dir("hello").exists());
    assert!(!mcp.paths.logs_dir().join("plugin-hello.log").exists());

    let error = unwrap_json(
        mcp.plugin_remove(Parameters(PluginIdParams {
            id: "hello".to_string(),
        }))
        .await,
    )
    .expect_err("a second delete has nothing left");
    assert!(error.message.contains("no plugin"), "{}", error.message);
}

/// Editing a deactivated plugin's code must keep the file and say plainly
/// that nothing started — an agent that is not told will keep debugging a
/// plugin that was never meant to run.
#[tokio::test]
async fn writing_to_a_deactivated_plugin_stores_the_file_and_reports_why_it_did_not_start() {
    let (_dir, mcp) = test_handler().await;
    install(&mcp, "hello");
    unwrap_json(mcp.plugin_set_active(set_active("hello", false)).await).expect("deactivate");

    let written = unwrap_json(
        mcp.plugin_write_file(Parameters(super::plugin_types::PluginWriteFileParams {
            id: "hello".to_string(),
            path: "notes.txt".to_string(),
            content: "kept".to_string(),
        }))
        .await,
    )
    .expect("write");

    assert_eq!(
        fs::read_to_string(mcp.paths.plugins_dir().join("hello/notes.txt")).expect("read back"),
        "kept",
        "the write itself must still happen"
    );
    assert!(
        written.message.contains("DEACTIVATED"),
        "{}",
        written.message
    );
    assert!(
        written.message.contains("plugin_set_active"),
        "it must name the way to run it again: {}",
        written.message
    );
}

/// Every capability an agent cannot see does not exist. These descriptions
/// are the only documentation a fresh session gets.
#[test]
fn the_tool_descriptions_separate_the_three_actions() {
    let tools = super::SmabarMcp::lifecycle_tool_router().list_all();
    let by_name = |name: &str| -> String {
        tools
            .iter()
            .find(|tool| tool.name == name)
            .and_then(|tool| tool.description.clone())
            .unwrap_or_else(|| panic!("tool {name} has no description"))
            .to_string()
    };

    let deactivate = by_name("plugin_set_active");
    assert!(deactivate.contains("plugin_set_visible"));
    assert!(deactivate.contains("plugin_remove"));
    assert!(deactivate.contains("kept"), "it must say nothing is lost");

    let hide = by_name("plugin_set_visible");
    assert!(hide.contains("keeps running"));
    assert!(hide.contains("plugin_set_active"));

    let remove = by_name("plugin_remove");
    assert!(
        remove.contains("CANNOT BE UNDONE"),
        "deletion must state plainly that it is irreversible: {remove}"
    );
    assert!(remove.contains("data directory"));
    assert!(remove.contains("log"));
    assert!(
        remove.contains("plugin_set_active"),
        "it must name the reversible alternative"
    );
}
