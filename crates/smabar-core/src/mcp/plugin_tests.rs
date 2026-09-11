//! MCP tests for the plugin tools: writing, validating, reading and logging.
//!
//! Split out of `tests.rs` to stay under the line limit; the shared
//! `test_handler`/`unwrap_json` helpers still live there.

use std::{fs, path::PathBuf};

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use super::plugin_types::{
    PluginActionParams, PluginIdParams, PluginLogsParams, PluginWriteFileParams,
};
use super::tests::{test_handler, unwrap_json};

fn write_params(id: &str, path: &str, content: &str) -> Parameters<PluginWriteFileParams> {
    Parameters(PluginWriteFileParams {
        id: id.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    })
}

#[tokio::test]
async fn plugin_write_file_rejects_path_traversal_and_bad_ids() {
    let (_dir, mcp) = test_handler().await;
    for path in ["../evil.py", "/etc/passwd", "a/../../evil.py", "", ".."] {
        let err = unwrap_json(
            mcp.plugin_write_file(write_params("hello", path, "x"))
                .await,
        )
        .expect_err(&format!("path {path:?} must be rejected"));
        assert!(
            err.message.contains("plugin folder"),
            "path {path:?}: {}",
            err.message
        );
    }
    for id in ["", "Has-Upper", "under_score", "dot.ted", "../up"] {
        let err = unwrap_json(
            mcp.plugin_write_file(write_params(id, "plugin.py", "x"))
                .await,
        )
        .expect_err(&format!("id {id:?} must be rejected"));
        assert!(
            err.message.contains("[a-z0-9-]"),
            "id {id:?}: {}",
            err.message
        );
    }
    // Nothing may have been created for the rejected writes.
    assert!(!mcp.paths.plugins_dir().join("hello").exists());
}

#[tokio::test]
async fn plugin_write_then_read_roundtrip_creates_a_new_plugin_folder() {
    let (_dir, mcp) = test_handler().await;
    // exec runtime: no uv needed, so the plugin really starts inside the test.
    let manifest = r#"{"id":"hello","name":"Hello","version":"1","protocolVersion":1,
"runtime":"exec","command":["true"],"tiles":[{"id":"greeting","name":"Greeting",
"iconSvg":"<svg viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"10\"/></svg>"}]}"#;
    let script = "print('hi')\n";

    // The entry script comes first: no manifest yet, so nothing can start and
    // the tool says so instead of burning the reload timeout.
    let script_write = unwrap_json(
        mcp.plugin_write_file(write_params("hello", "sub/./plugin.py", script))
            .await,
    )
    .expect("write nested script");
    assert!(script_write.reload.is_none());
    assert!(script_write.message.contains("manifest LAST"));

    let written = unwrap_json(
        mcp.plugin_write_file(write_params("hello", "smabar.json", manifest))
            .await,
    )
    .expect("write manifest");
    let reload = written
        .reload
        .expect("manifest write reports the plugin state");
    assert_eq!(reload.tiles, vec!["greeting".to_string()]);
    assert!(
        reload.design_review.is_none(),
        "failed startup needs runtime recovery first"
    );

    let result = unwrap_json(
        mcp.plugin_read(Parameters(PluginIdParams {
            id: "hello".to_string(),
        }))
        .await,
    )
    .expect("read plugin folder");
    assert_eq!(
        result.dir,
        mcp.paths.plugins_dir().join("hello").display().to_string()
    );
    // The atomic write's dot-prefixed temp file must never linger.
    let paths: Vec<PathBuf> = result
        .files
        .iter()
        .map(|file| PathBuf::from(&file.path))
        .collect();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("smabar.json"),
            PathBuf::from("sub").join("plugin.py"),
        ]
    );
    assert_eq!(result.files[0].content.as_deref(), Some(manifest));
    assert_eq!(result.files[1].content.as_deref(), Some(script));
    assert!(!result.files[0].truncated);
    assert!(!result.files[0].binary);

    let listed = unwrap_json(mcp.plugin_list().await).expect("list plugin");
    let tile = listed.plugins[0]
        .tiles
        .as_ref()
        .and_then(|tiles| tiles.first())
        .expect("listed tile");
    assert_eq!(
        tile.icon_svg.as_deref(),
        Some("<svg viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"10\"/></svg>")
    );
}

#[tokio::test]
async fn a_broken_manifest_is_rejected_and_never_written() {
    let (_dir, mcp) = test_handler().await;
    let error = unwrap_json(
        mcp.plugin_write_file(write_params("hello", "smabar.json", r#"{ "id": "hello" }"#))
            .await,
    )
    .expect_err("an incomplete manifest must be refused");
    assert!(error.message.contains("name"), "{}", error.message);
    assert!(
        !mcp.paths
            .plugins_dir()
            .join("hello")
            .join("smabar.json")
            .exists(),
        "a rejected manifest must not reach the plugins folder"
    );
}

#[tokio::test]
async fn a_manifest_id_must_match_its_folder() {
    let (_dir, mcp) = test_handler().await;
    let manifest = r#"{"id":"other","name":"Other","version":"1","protocolVersion":1,
"runtime":"exec","command":["true"],"tiles":[{"id":"w","name":"W"}]}"#;
    let error = unwrap_json(
        mcp.plugin_write_file(write_params("hello", "smabar.json", manifest))
            .await,
    )
    .expect_err("a mismatched id must be refused");
    assert!(
        error.message.contains("does not match"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn plugin_read_unknown_plugin_is_an_error() {
    let (_dir, mcp) = test_handler().await;
    let err = unwrap_json(
        mcp.plugin_read(Parameters(PluginIdParams {
            id: "ghost".to_string(),
        }))
        .await,
    )
    .expect_err("unknown plugin");
    assert!(err.message.contains("ghost"));
}

#[test]
fn plugin_action_schema_and_description_explain_the_async_contract() {
    let schema = schemars::schema_for!(PluginActionParams);
    let schema = schema.as_value();
    let required = schema["required"].as_array().expect("required fields");
    for field in ["id", "tile", "action"] {
        assert!(
            required.iter().any(|entry| entry == field),
            "missing {field}"
        );
    }
    assert!(!required.iter().any(|entry| entry == "value"));
    assert_eq!(
        schema
            .pointer("/properties/value/type")
            .expect("value property carries every JSON type"),
        &json!(["object", "array", "string", "number", "boolean", "null"])
    );

    let description = super::SmabarMcp::plugin_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "plugin_action")
        .and_then(|tool| tool.description)
        .expect("plugin_action is described");
    for detail in ["QUEUED", "manifest-local", "value", "plugin_list"] {
        assert!(description.contains(detail), "description misses {detail}");
    }
}

#[tokio::test]
async fn plugin_action_forwards_an_optional_json_value() {
    let (_dir, mcp) = test_handler().await;
    let script = r#"import json
import sys

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        send({"jsonrpc": "2.0", "id": message["id"], "result": {}})
    elif message.get("method") == "event":
        send({"jsonrpc": "2.0", "method": "ui.render", "params": {
            "tileId": "switcher", "target": "flyout",
            "html": json.dumps(message["params"], sort_keys=True)}})
    elif message.get("method") == "shutdown":
        send({"jsonrpc": "2.0", "id": message["id"], "result": {}})
        break
"#;
    let manifest = r#"{"id":"action-test","name":"Action Test","version":"1",
"protocolVersion":1,"runtime":"exec","command":["python3","main.py"],
"tiles":[{"id":"switcher","name":"Switcher","hasFlyout":true}]}"#;
    unwrap_json(
        mcp.plugin_write_file(write_params("action-test", "main.py", script))
            .await,
    )
    .expect("write action fixture");
    let written = unwrap_json(
        mcp.plugin_write_file(write_params("action-test", "smabar.json", manifest))
            .await,
    )
    .expect("start action fixture");
    assert!(written.message.contains("visual review"));
    let review = written
        .reload
        .expect("reload")
        .design_review
        .expect("design review");
    assert_eq!(review["status"], "pending");
    assert_eq!(review["read"]["tool"], "ui_kit");
    let params = serde_json::from_value(review["read"]["arguments"].clone())
        .expect("review points to valid UI-kit parameters");
    let kit = unwrap_json(mcp.ui_kit(Parameters(params)).await).expect("review guidance");
    assert!(kit.best_practices.is_some() && kit.snippets.is_some());
    let calls = &review["tiles"][0]["calls"];
    assert_eq!(
        calls[0]["arguments"]["target"],
        "plugin:action-test:switcher"
    );
    assert_eq!(calls[1]["arguments"]["action"], "open_flyout");
    assert_eq!(
        calls[1]["arguments"]["tileId"],
        "plugin:action-test:switcher"
    );
    assert_eq!(calls[2]["arguments"]["target"], "flyout");

    let mut events = mcp.supervisor.subscribe_events();
    let ack = unwrap_json(
        mcp.plugin_action(Parameters(PluginActionParams {
            id: "action-test".to_string(),
            tile: "switcher".to_string(),
            action: "next".to_string(),
            value: Some(json!({"page": 2})),
        }))
        .await,
    )
    .expect("queue action");
    let observed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(crate::plugins::PluginEvent::UiRender { target, html, .. }) =
                events.recv().await
                && target == "flyout"
            {
                break html;
            }
        }
    })
    .await;
    mcp.supervisor.shutdown_all().await;

    assert!(ack.message.contains("queued action \"next\""));
    let html = observed.expect("plugin did not handle the action");
    assert!(html.contains(r#""action": "next""#), "{html}");
    assert!(html.contains(r#""value": {"page": 2}"#), "{html}");
}

#[tokio::test]
async fn plugin_action_unknown_plugin_points_to_plugin_list() {
    let (_dir, mcp) = test_handler().await;
    let error = unwrap_json(
        mcp.plugin_action(Parameters(PluginActionParams {
            id: "ghost".to_string(),
            tile: "status".to_string(),
            action: "refresh".to_string(),
            value: None,
        }))
        .await,
    )
    .expect_err("unknown plugin");
    assert!(error.message.contains("ghost"));
    assert!(error.message.contains("plugin_list"));
}

#[tokio::test]
async fn plugin_logs_parses_filters_and_limits_the_fixture_file() {
    let (_dir, mcp) = test_handler().await;
    let logs_dir = mcp.paths.logs_dir();
    fs::create_dir_all(&logs_dir).expect("create logs dir");
    fs::write(
        logs_dir.join("plugin-hello.log"),
        concat!(
            r#"{"ts":1,"level":"info","source":"log","message":"boot"}"#,
            "\n",
            "not json at all\n",
            r#"{"ts":2,"level":"wat","source":"stdout","message":"strange level"}"#,
            "\n",
            r#"{"ts":3,"level":"warn","source":"stderr","message":"low disk","fields":{"percent":9}}"#,
            "\n",
            r#"{"ts":4,"level":"error","source":"log","message":"crashed"}"#,
            "\n",
        ),
    )
    .expect("write fixture log");

    let params = |level: Option<&str>, limit: Option<usize>, contains: Option<&str>| {
        Parameters(PluginLogsParams {
            id: Some("hello".to_string()),
            level: level.map(str::to_string),
            limit,
            contains: contains.map(str::to_string),
        })
    };

    // No filter: every parseable entry (unknown level included), 1 skipped.
    let all = unwrap_json(mcp.plugin_logs(params(None, None, None)).await).expect("query all");
    let entries = all.plugin_entries.expect("plugin entries");
    let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        vec!["boot", "strange level", "low disk", "crashed"]
    );
    assert_eq!(all.skipped_lines, 1);
    assert_eq!(entries[2].fields, Some(json!({ "percent": 9 })));

    // Level filter drops lower levels AND unknown level strings.
    let warnings =
        unwrap_json(mcp.plugin_logs(params(Some("warn"), None, None)).await).expect("query warn");
    let entries = warnings.plugin_entries.expect("plugin entries");
    let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, vec!["low disk", "crashed"]);

    // Substring matches the raw line, field values included.
    let by_field = unwrap_json(mcp.plugin_logs(params(None, None, Some("percent"))).await)
        .expect("query contains");
    let entries = by_field.plugin_entries.expect("plugin entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "low disk");

    // Limit keeps the newest entries.
    let newest =
        unwrap_json(mcp.plugin_logs(params(None, Some(2), None)).await).expect("query limit");
    let entries = newest.plugin_entries.expect("plugin entries");
    let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, vec!["low disk", "crashed"]);

    // Invalid level parameter is an error listing the valid levels.
    let err = unwrap_json(mcp.plugin_logs(params(Some("loud"), None, None)).await)
        .expect_err("invalid level");
    assert!(err.message.contains("trace, debug, info, warn, error"));
}

#[tokio::test]
async fn plugin_logs_missing_file_is_empty_not_an_error() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(
        mcp.plugin_logs(Parameters(PluginLogsParams {
            id: Some("ghost".to_string()),
            level: None,
            limit: None,
            contains: None,
        }))
        .await,
    )
    .expect("missing log file");
    assert_eq!(result.plugin_entries.expect("plugin entries").len(), 0);
    assert_eq!(result.skipped_lines, 0);
}
