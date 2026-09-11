//! `plugin_logs` as a diagnosis tool: one call must show everything about a
//! plugin, from its own log AND from what the core said about it.
//!
//! Split out of `plugin_tests.rs` for the 500-line limit.

use std::fs;

use rmcp::handler::server::wrapper::Parameters;

use super::plugin_types::PluginLogsParams;
use super::tests::{test_handler, unwrap_json};

fn params(id: &str) -> Parameters<PluginLogsParams> {
    Parameters(PluginLogsParams {
        id: Some(id.to_string()),
        level: None,
        limit: None,
        contains: None,
    })
}

/// The core log as tracing writes it, with the structured `plugin` field.
fn write_core_log(logs_dir: &std::path::Path) {
    fs::create_dir_all(logs_dir).expect("create logs dir");
    fs::write(
        logs_dir.join("smabar.log.2026-08-23"),
        concat!(
            r#"{"timestamp":"2026-08-23T10:00:00Z","level":"ERROR","fields":{"message":"plugin run failed","plugin":"clock","reason":"SyntaxError"},"target":"smabar_core::plugins::runner"}"#,
            "\n",
            r#"{"timestamp":"2026-08-23T10:00:01Z","level":"INFO","fields":{"message":"plugin running","plugin":"clockwork"},"target":"smabar_core::plugins::runner"}"#,
            "\n",
            r#"{"timestamp":"2026-08-23T10:00:02Z","level":"INFO","fields":{"message":"a note mentioning clock in prose"},"target":"smabar_core::app"}"#,
            "\n",
        ),
    )
    .expect("write core log");
}

#[tokio::test]
async fn one_call_returns_the_plugin_log_and_what_the_core_said_about_it() {
    let (_dir, mcp) = test_handler().await;
    let logs_dir = mcp.paths.logs_dir();
    write_core_log(&logs_dir);
    fs::write(
        logs_dir.join("plugin-clock.log"),
        concat!(
            r#"{"ts":1,"level":"info","source":"log","message":"started"}"#,
            "\n",
            r#"{"ts":2,"level":"warn","source":"shell","message":"markup removed from \"world\" (flyout): form"}"#,
            "\n",
        ),
    )
    .expect("write plugin log");

    let result = unwrap_json(mcp.plugin_logs(params("clock")).await).expect("query");

    let plugin = result.plugin_entries.expect("plugin entries");
    assert_eq!(plugin.len(), 2);
    // The shell's report about dropped markup arrives in the PLUGIN's log —
    // that is where a plugin author's agent looks.
    assert_eq!(plugin[1].source, "shell");

    let core = result.core_entries.expect("core entries");
    assert_eq!(core.len(), 1, "only entries tagged plugin=clock: {core:?}");
    assert_eq!(core[0].message, "plugin run failed");
}

#[tokio::test]
async fn a_plugin_that_died_before_writing_still_explains_itself() {
    // The case that made the merge necessary: no plugin log file at all, and
    // the reason lives entirely in the core log.
    let (_dir, mcp) = test_handler().await;
    write_core_log(&mcp.paths.logs_dir());

    let result = unwrap_json(mcp.plugin_logs(params("clock")).await).expect("query");

    assert!(result.plugin_entries.expect("plugin entries").is_empty());
    let core = result.core_entries.expect("core entries");
    assert_eq!(core.len(), 1);
    assert_eq!(core[0].fields["reason"], "SyntaxError");
}

#[tokio::test]
async fn a_similar_plugin_name_or_a_passing_mention_is_not_a_match() {
    // Field equality, not substring: "clockwork" and prose containing the
    // word "clock" must stay out of clock's diagnosis.
    let (_dir, mcp) = test_handler().await;
    write_core_log(&mcp.paths.logs_dir());

    let clockwork = unwrap_json(mcp.plugin_logs(params("clockwork")).await).expect("query");
    let core = clockwork.core_entries.expect("core entries");
    assert_eq!(core.len(), 1);
    assert_eq!(core[0].message, "plugin running");
}

#[tokio::test]
async fn without_an_id_the_core_log_is_returned_whole() {
    let (_dir, mcp) = test_handler().await;
    write_core_log(&mcp.paths.logs_dir());

    let result = unwrap_json(
        mcp.plugin_logs(Parameters(PluginLogsParams {
            id: None,
            level: None,
            limit: None,
            contains: None,
        }))
        .await,
    )
    .expect("query");

    assert!(result.plugin_entries.is_none());
    assert_eq!(result.core_entries.expect("core entries").len(), 3);
}

/// The files that WRITE plugin log entries, read at compile time. Every
/// `source` string a plugin author can encounter originates in one of them.
const RUNNER_SOURCE: &str = include_str!("../plugins/runner.rs");
const HANDLERS_SOURCE: &str = include_str!("../plugins/handlers.rs");
const LOGFILE_SOURCE: &str = include_str!("../plugins/logfile.rs");
const COMMANDS_SOURCE: &str = include_str!("../../../smabar/src/commands/mod.rs");

/// One list of log sources, kept in step in the four places an agent reads
/// them: the code that writes them, the tool description, the entry type's
/// schema and the guide. Both failure modes happened once — a source was
/// documented before anything wrote it, and a rewrite dropped one that very
/// much existed.
#[test]
fn every_log_source_is_written_and_explained_in_one_place() {
    let code = format!("{RUNNER_SOURCE}{HANDLERS_SOURCE}{LOGFILE_SOURCE}{COMMANDS_SOURCE}");
    let description = super::SmabarMcp::plugin_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "plugin_logs")
        .and_then(|tool| tool.description.map(|d| d.to_string()))
        .expect("plugin_logs is described");
    let schema = serde_json::to_value(schemars::schema_for!(super::plugin_types::PluginLogEntry))
        .expect("schema");
    let source_doc = schema["properties"]["source"]["description"]
        .as_str()
        .expect("source is documented");
    let guide = super::guide_tools::GUIDE["debugging"]["logs"]
        .as_str()
        .expect("debugging.logs is documented");
    for source in crate::plugins::LOG_SOURCES {
        let quoted = format!("\"{source}\"");
        assert!(
            code.contains(&quoted),
            "no writer produces source {source:?} any more"
        );
        assert!(
            description.contains(&quoted),
            "plugin_logs never explains {source:?}"
        );
        assert!(
            source_doc.contains(&format!("`{source}`")),
            "PluginLogEntry.source misses {source}"
        );
        assert!(
            guide.contains(&quoted),
            "guide debugging.logs misses {source:?}"
        );
    }
    assert!(guide.contains("pluginEntries") && guide.contains("coreEntries"));
}
