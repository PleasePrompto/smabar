//! What `logging::init` actually captures, proven end to end.
//!
//! An integration test on purpose: `init` installs a GLOBAL subscriber, so it
//! can only be exercised once per process. Everything therefore lives in one
//! test function, and cargo gives this file its own binary.

use std::fs;

use smabar_core::config::SmabarPaths;
use smabar_core::logging::{self, LogFilter, LogLevel};

/// Reads every log line the run produced, as raw JSON.
fn written_lines(paths: &SmabarPaths) -> Vec<serde_json::Value> {
    let dir = paths.logs_dir();
    let mut lines = Vec::new();
    for entry in fs::read_dir(&dir).expect("logs dir").flatten() {
        let text = fs::read_to_string(entry.path()).expect("read log file");
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            lines.push(serde_json::from_str(line).expect("log lines are JSON"));
        }
    }
    lines
}

fn messages(lines: &[serde_json::Value]) -> Vec<&str> {
    lines
        .iter()
        .filter_map(|line| line["fields"]["message"].as_str())
        .collect()
}

#[test]
fn the_log_captures_dependencies_and_damps_the_mcp_transport() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = SmabarPaths::new(temp.path().join("smabar"));
    // SMABAR_LOG must not leak in from the developer's shell.
    unsafe { std::env::remove_var("SMABAR_LOG") };

    let guard = logging::init(&paths).expect("init logging");

    // 1. Our own events, the baseline.
    tracing::info!("from tracing");
    // 2. A dependency logging through the `log` crate. This is the bridge:
    //    Tauri reports "asset protocol not configured to allow the path" this
    //    way, and without the bridge it was invisible while debugging exactly
    //    that.
    log::error!("from the log crate");
    // 3. The MCP transport's per-request bookkeeping — three of these per
    //    tool call, which is what buries a real failure.
    tracing::info!(target: "rmcp::service", "serve finished");
    // 4. …but a transport WARNING must still get through.
    tracing::warn!(target: "rmcp::transport", "rejected request with disallowed Origin header");
    // 5. Below the default level: dropped.
    tracing::debug!("noisy detail");
    // 6. A missing configured locale must explain both supported recovery paths.
    let _fallback = smabar_core::i18n::resolve(&paths, "fr");

    // Dropping the guard flushes the non-blocking writer.
    drop(guard);

    let lines = written_lines(&paths);
    let messages = messages(&lines);
    assert!(messages.contains(&"from tracing"), "{messages:?}");
    assert!(
        messages.contains(&"from the log crate"),
        "the log -> tracing bridge is gone: {messages:?}"
    );
    assert!(
        !messages.contains(&"serve finished"),
        "rmcp info must be damped: {messages:?}"
    );
    assert!(
        messages.contains(&"rejected request with disallowed Origin header"),
        "rmcp warnings must survive the damping: {messages:?}"
    );
    assert!(!messages.contains(&"noisy detail"), "{messages:?}");
    let locale_warning = messages
        .iter()
        .find(|message| message.contains("configured locale file is missing"))
        .expect("missing configured locale must be observable in the central log");
    assert!(
        locale_warning.contains("add a flat JSON locale file")
            && locale_warning.contains("set \"language\" to \"en\" or \"de\""),
        "locale warning must name both supported recovery paths: {locale_warning}"
    );

    // A bridged record must be a first-class entry for the MCP log tools,
    // not just a line in a file.
    let queried = logging::query(
        &paths,
        &LogFilter {
            min_level: Some(LogLevel::Error),
            substring: None,
            plugin: None,
            limit: None,
        },
    )
    .expect("query logs");
    let bridged = queried
        .entries
        .iter()
        .find(|entry| entry.message == "from the log crate")
        .expect("the bridged record is queryable");
    assert_eq!(bridged.level, LogLevel::Error);
    assert_eq!(bridged.target, "logging");
}
