//! What a plugin START tells the agent through `plugin_write_file`: the
//! manifest gate, the lifted crash cause, and the warnings/render facts a
//! reply carries so a broken start cannot pass as a clean one.

use rmcp::handler::server::wrapper::Parameters;

use super::plugin_types::{PluginWriteFileParams, PluginWriteResult};
use super::tests::{test_handler, unwrap_json};

fn write_params(id: &str, path: &str, content: &str) -> Parameters<PluginWriteFileParams> {
    Parameters(PluginWriteFileParams {
        id: id.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    })
}

/// An exec fixture that answers initialize with a render into every tile
/// named in `tiles`, then serves until shutdown.
fn rendering_fixture(tiles: &[&str]) -> String {
    let renders: String = tiles
        .iter()
        .map(|tile| {
            format!(
                "        send({{\"jsonrpc\": \"2.0\", \"method\": \"ui.render\", \"params\": {{\
                 \"tileId\": \"{tile}\", \"target\": \"tile\", \"html\": \"<b>{tile}</b>\"}}}})\n"
            )
        })
        .collect();
    format!(
        r#"import json
import sys

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        send({{"jsonrpc": "2.0", "id": message["id"], "result": {{}}}})
{renders}    elif message.get("method") == "shutdown":
        send({{"jsonrpc": "2.0", "id": message["id"], "result": {{}}}})
        break
"#
    )
}

fn exec_manifest(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","name":"Start","version":"1","protocolVersion":1,"runtime":"exec",
"command":["python3","main.py"],"tiles":[{{"id":"w","name":"W"}}]}}"#
    )
}

async fn start(mcp: &super::SmabarMcp, id: &str, script: &str) -> PluginWriteResult {
    unwrap_json(
        mcp.plugin_write_file(write_params(id, "main.py", script))
            .await,
    )
    .expect("write fixture");
    unwrap_json(
        mcp.plugin_write_file(write_params(id, "smabar.json", &exec_manifest(id)))
            .await,
    )
    .expect("write manifest")
}

#[tokio::test]
async fn a_python_manifest_without_its_entry_script_is_refused_and_never_written() {
    let (_dir, mcp) = test_handler().await;
    let manifest = r#"{"id":"hello","name":"Hello","version":"1","protocolVersion":1,
"runtime":"python","entry":"plugin.py","tiles":[{"id":"w","name":"W"}]}"#;
    let error = unwrap_json(
        mcp.plugin_write_file(write_params("hello", "smabar.json", manifest))
            .await,
    )
    .expect_err("a manifest whose entry script is missing must be refused");
    assert!(
        error
            .message
            .contains("plugin_write_file(path=\"plugin.py\")"),
        "the refusal names the fix: {}",
        error.message
    );
    assert!(
        error.message.contains("NOT written"),
        "the refusal says nothing changed: {}",
        error.message
    );
    assert!(
        !mcp.paths
            .plugins_dir()
            .join("hello")
            .join("smabar.json")
            .exists(),
        "a refused manifest never reaches the folder"
    );
}

#[tokio::test]
async fn a_crashing_entry_script_reports_its_traceback_cause() {
    let (_dir, mcp) = test_handler().await;
    let written = start(&mcp, "crashing", "import views\n").await;
    let reload = written.reload.expect("reload");
    assert_eq!(reload.status, crate::plugins::PluginStatus::Failed);
    let error = reload.error.expect("a failed start carries its reason");
    assert!(
        error.starts_with("ModuleNotFoundError: No module named 'views'"),
        "the traceback's last line leads the reason: {error}"
    );
    assert!(
        written.message.contains("ModuleNotFoundError"),
        "{}",
        written.message
    );
    assert!(
        reload
            .warnings
            .messages
            .iter()
            .any(|message| message.starts_with("stderr: ModuleNotFoundError")),
        "warnings: {:?}",
        reload.warnings.messages
    );
    let listed = unwrap_json(mcp.plugin_list().await).expect("list");
    let entry = listed
        .plugins
        .iter()
        .find(|plugin| plugin.id == "crashing")
        .expect("listed");
    assert!(
        entry
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ModuleNotFoundError")),
        "plugin_list carries the same cause: {:?}",
        entry.error
    );
    mcp.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_render_into_an_undeclared_tile_is_a_warning_the_reply_cannot_hide() {
    let (_dir, mcp) = test_handler().await;
    let written = start(&mcp, "ghostly", &rendering_fixture(&["w", "ghost"])).await;
    let reload = written.reload.expect("reload");
    assert_eq!(reload.status, crate::plugins::PluginStatus::Running);
    assert_eq!(reload.rendered, vec!["w".to_string()]);
    assert_eq!(reload.warnings.count, 1, "{:?}", reload.warnings.messages);
    assert!(
        reload.warnings.messages[0].starts_with("core: ui.render dropped"),
        "{:?}",
        reload.warnings.messages
    );
    assert!(
        written.message.contains("1 warning(s)"),
        "the message counts the warnings: {}",
        written.message
    );
    let review = reload.design_review.expect("review");
    assert_eq!(review["done"][0]["met"], false);
    assert_eq!(review["done"][1]["met"], true);
    mcp.supervisor.shutdown_all().await;
}

#[tokio::test]
async fn a_clean_start_says_so_and_ticks_the_host_checked_criteria() {
    let (_dir, mcp) = test_handler().await;
    let written = start(&mcp, "clean", &rendering_fixture(&["w"])).await;
    let reload = written.reload.expect("reload");
    assert_eq!(reload.warnings.count, 0, "{:?}", reload.warnings.messages);
    assert_eq!(reload.rendered, vec!["w".to_string()]);
    assert!(
        written.message.contains("clean start"),
        "{}",
        written.message
    );
    assert!(
        written.message.contains("visual review"),
        "{}",
        written.message
    );
    let review = reload.design_review.expect("review");
    assert_eq!(review["done"][0]["met"], true);
    assert_eq!(review["done"][1]["met"], true);
    mcp.supervisor.shutdown_all().await;
}
