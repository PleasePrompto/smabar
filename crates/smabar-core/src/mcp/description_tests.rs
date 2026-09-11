//! Tool descriptions sit in every client's context. The ones an agent reads
//! on every build carry a word budget, so detail moves to the guide or the
//! contract instead of settling here.

use super::tests::test_handler;

#[tokio::test]
async fn the_core_tool_descriptions_stay_inside_their_word_budget() {
    let (_dir, mcp) = test_handler().await;
    for tool in mcp.tool_router.list_all() {
        let budget = match tool.name.as_ref() {
            "ui_kit" | "plugin_write_file" | "plugin_logs" | "plugin_guide" => 220,
            _ => continue,
        };
        let words = tool
            .description
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .count();
        assert!(
            words <= budget,
            "{} grew to {words} words (budget {budget}); move detail to its owner",
            tool.name
        );
    }
}
