//! Discovery and content tests for smabar's guided design prompt.

use rmcp::ServerHandler;
use rmcp::model::Role;

use super::tests::test_handler;

#[tokio::test]
async fn design_plugin_prompt_is_discoverable_and_complete() {
    let (_dir, mcp) = test_handler().await;
    let info = mcp.get_info();
    assert!(info.capabilities.tools.is_some());
    assert!(info.capabilities.prompts.is_some());

    let prompts = mcp.prompt_router.list_all();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "design_plugin");
    assert!(prompts[0].description.is_some());
    assert!(prompts[0].arguments.is_none());

    let messages = mcp.design_plugin();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, Role::User);
    let text = &messages[0].content.as_text().expect("text prompt").text;
    for needle in [
        "plugin_guide",
        "sections",
        "coverLayouts",
        "bestPractices",
        "hasFlyout",
        "plugin_logs",
        "bar_ui_state",
        "open_flyout",
        "bar_screenshot",
    ] {
        assert!(text.contains(needle), "design prompt misses {needle}");
    }
    // One source: every golden-path step appears verbatim, criterion included.
    for step in super::guide_tools::GUIDE["goldenPath"]
        .as_array()
        .expect("steps")
    {
        for key in ["do", "done"] {
            let sentence = step[key].as_str().expect("step text");
            assert!(
                text.contains(sentence),
                "prompt misses step {} {key}",
                step["step"]
            );
        }
    }
}
