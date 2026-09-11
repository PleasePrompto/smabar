//! A new MCP client can discover the host without any optional plugin installed.
use rmcp::{ServerHandler, handler::server::wrapper::Parameters};
use serde_json::json;

use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};

#[tokio::test]
async fn empty_installation_still_explains_capabilities_and_valid_next_calls() {
    let (_dir, mcp) = test_handler().await;
    assert!(mcp.supervisor.plugin_infos().is_empty());
    let guide = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("capabilities".into()),
        }))
        .await,
    )
    .expect("guide");
    assert_eq!(
        guide.capabilities.as_ref().expect("capabilities")["runtime"]["availableProviders"],
        json!(mcp.hub.available_names())
    );
    assert_eq!(
        guide.capabilities.as_ref().expect("capabilities")["runtime"]["services"],
        json!(["commands"])
    );
    assert!(
        guide.sdk.is_none(),
        "capabilities is an index, not the full API"
    );
    let tools = mcp.tool_router.list_all();
    let topics = guide.capabilities.as_ref().expect("capabilities")["topics"]
        .as_array()
        .expect("topics");
    for required in [
        "surfaces",
        "design",
        "interactions",
        "notifications",
        "audioPlayback",
        "mediaPipeline",
        "systemData",
        "persistenceAndScheduling",
        "agentCommands",
        "integrations",
        "verification",
    ] {
        assert!(
            topics.iter().any(|topic| topic["id"] == required),
            "missing {required}"
        );
    }
    for topic in topics {
        for next in topic["read"].as_array().expect("follow-up calls") {
            let name = next["tool"].as_str().expect("tool name");
            assert!(
                tools.iter().any(|tool| tool.name == name),
                "unknown pointer {name}"
            );
            match name {
                "plugin_guide" => {
                    let params = serde_json::from_value(next["arguments"].clone())
                        .expect("guide parameters");
                    mcp.plugin_guide(Parameters(params))
                        .await
                        .expect("guide section exists");
                }
                "ui_kit" => {
                    let params =
                        serde_json::from_value(next["arguments"].clone()).expect("kit parameters");
                    mcp.ui_kit(Parameters(params))
                        .await
                        .expect("UI references resolve");
                }
                _ => {}
            }
        }
    }
    let instructions = mcp.get_info().instructions.expect("server instructions");
    for detail in [
        "user supplies the idea",
        "capabilities",
        "no bundled plugin is required",
        "official",
        "plugin_commands",
    ] {
        assert!(
            instructions.contains(detail),
            "new client cannot discover {detail}"
        );
    }
    assert!(!instructions.contains("todos"));
    let command = tools
        .iter()
        .find(|tool| tool.name == "plugin_call")
        .expect("plugin_call");
    assert_eq!(
        command.input_schema["properties"]["arguments"]["type"],
        "object"
    );
}
