//! Publishing guidance stays optional, discoverable and limited to new creations.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use super::guide_tools::PUBLISHING_HINT;
use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};

#[tokio::test]
async fn publishing_is_one_shared_reference_for_plugins_and_themes() {
    let (_dir, mcp) = test_handler().await;
    let start = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams { section: None }))
            .await,
    )
    .expect("start guide");
    assert!(start.sections.expect("index")["publishing"].is_string());
    assert!(
        start.publishing.is_none(),
        "details stay out of the start reply"
    );
    assert!(
        start
            .golden_path
            .expect("steps")
            .to_string()
            .contains("publishing")
    );
    let mut reference = None;
    for section in ["publishing", "all"] {
        let reply = unwrap_json(
            mcp.plugin_guide(Parameters(GuideParams {
                section: Some(section.to_string()),
            }))
            .await,
        )
        .expect("publishing guide");
        let publishing = reply.publishing.expect("reference");
        assert_eq!(publishing["submit"]["method"], "POST");
        assert_eq!(
            publishing["submit"]["url"],
            "https://store.smabar.com/submit"
        );
        assert_eq!(
            publishing["submit"]["body"],
            json!({"url": "https://github.com/<owner>/<repo>"})
        );
        for (field, facts) in [
            (
                "offer",
                &[
                    "once",
                    "declined",
                    "final functional and visual checks",
                    "routine edit/reload",
                ][..],
            ),
            (
                "approval",
                &["explicit user authorization", "license", "app.data_dir"][..],
            ),
        ] {
            let text = publishing[field].as_str().expect("guidance");
            for fact in facts {
                assert!(text.contains(fact), "{field}: missing {fact}");
            }
        }
        assert!(
            publishing["required"]["plugin"]
                .as_str()
                .expect("plugin")
                .contains("smabar.json")
        );
        assert!(
            publishing["required"]["theme"]
                .as_str()
                .expect("theme")
                .contains("meta.version")
        );
        if let Some(previous) = &reference {
            assert_eq!(&publishing, previous);
        }
        reference = Some(publishing);
    }
    let tools = mcp.tool_router.list_all();
    for name in ["plugin_guide", "theme_get", "theme_write"] {
        let tool = tools.iter().find(|tool| tool.name == name).expect("tool");
        assert!(
            tool.description
                .as_deref()
                .expect("description")
                .contains("publishing")
        );
    }
}

#[tokio::test]
async fn a_theme_creation_hints_at_sharing_but_edits_do_not() {
    let (_dir, mcp) = test_handler().await;
    let original_config = mcp.config.current();
    for (tokens, created) in [
        (json!({"--sb-accent": "#123456"}), true),
        (json!({"--sb-accent": "#abcdef"}), false),
    ] {
        let reply = unwrap_json(
            mcp.theme_write(Parameters(
                serde_json::from_value(json!({
                    "name": "shareable", "tokens": tokens
                }))
                .expect("params"),
            ))
            .await,
        )
        .expect("write theme");
        assert_eq!(reply.message.contains(PUBLISHING_HINT), created);
        assert!(
            reply.message.contains("settings_set"),
            "activation hint preserved"
        );
    }
    assert_eq!(mcp.config.current(), original_config);
}
