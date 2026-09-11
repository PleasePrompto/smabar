//! Query API over the JSONL log files written by [`super::init`].

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{LOG_FILE_PREFIX, LoggingError};
use crate::config::SmabarPaths;

/// Log severity, ordered from `Trace` (lowest) to `Error` (highest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Parse a level name case-insensitively (`"info"`, `"WARN"`, …).
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// One parsed JSONL log line.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    /// All structured event fields except `message`.
    pub fields: serde_json::Value,
}

/// Filter for [`query`]; the default (empty) filter returns everything.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Keep only entries at or above this level.
    pub min_level: Option<LogLevel>,
    /// Keep only entries whose raw JSON line contains this substring
    /// (matches message, target, and field values alike).
    pub substring: Option<String>,
    /// Keep only entries carrying this exact `plugin` field.
    ///
    /// Deliberately not a substring match: everything the core says about
    /// plugin `clock` must not drag in `clockwork`, and a message merely
    /// mentioning the word must not count as a hit either.
    pub plugin: Option<String>,
    /// Keep only the newest N matching entries.
    pub limit: Option<usize>,
}

/// Matching entries (oldest first) plus the number of lines that could not
/// be parsed as log entries.
#[derive(Debug)]
pub struct LogQueryResult {
    pub entries: Vec<LogEntry>,
    pub skipped_lines: usize,
}

/// The subset of a `tracing-subscriber` JSON line we care about.
#[derive(Deserialize)]
struct RawLine {
    timestamp: String,
    level: String,
    target: String,
    #[serde(default)]
    fields: serde_json::Map<String, serde_json::Value>,
}

/// Read all `smabar.log*` files in `logs_dir()` (oldest first — the date
/// suffix sorts chronologically) and return the entries matching `filter`.
/// A missing logs directory yields an empty result, not an error.
pub fn query(paths: &SmabarPaths, filter: &LogFilter) -> Result<LogQueryResult, LoggingError> {
    let dir = paths.logs_dir();
    let mut files: Vec<PathBuf> = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(LOG_FILE_PREFIX)
                {
                    files.push(entry.path());
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LogQueryResult {
                entries: Vec::new(),
                skipped_lines: 0,
            });
        }
        Err(source) => return Err(LoggingError::Io { path: dir, source }),
    }
    files.sort();

    let mut entries = Vec::new();
    let mut skipped_lines = 0usize;
    for path in files {
        let file = File::open(&path).map_err(|source| LoggingError::Io {
            path: path.clone(),
            source,
        })?;
        for line in BufReader::new(file).lines() {
            let line = match line {
                Ok(line) => line,
                // A non-UTF-8 line is one bad line; anything else is a real
                // I/O failure and must surface.
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    skipped_lines += 1;
                    continue;
                }
                Err(source) => return Err(LoggingError::Io { path, source }),
            };
            if line.trim().is_empty() {
                continue;
            }
            match parse_line(&line) {
                Some(entry) => {
                    if matches(filter, &entry, &line) {
                        entries.push(entry);
                    }
                }
                None => skipped_lines += 1,
            }
        }
    }

    if let Some(limit) = filter.limit
        && entries.len() > limit
    {
        entries.drain(..entries.len() - limit);
    }
    Ok(LogQueryResult {
        entries,
        skipped_lines,
    })
}

fn parse_line(line: &str) -> Option<LogEntry> {
    let mut raw: RawLine = serde_json::from_str(line).ok()?;
    let level = LogLevel::parse(&raw.level)?;
    let message = match raw.fields.remove("message") {
        Some(serde_json::Value::String(text)) => text,
        Some(other) => other.to_string(),
        None => String::new(),
    };
    Some(LogEntry {
        timestamp: raw.timestamp,
        level,
        target: raw.target,
        message,
        fields: serde_json::Value::Object(raw.fields),
    })
}

fn matches(filter: &LogFilter, entry: &LogEntry, raw_line: &str) -> bool {
    if let Some(min) = filter.min_level
        && entry.level < min
    {
        return false;
    }
    if let Some(needle) = &filter.substring
        && !raw_line.contains(needle.as_str())
    {
        return false;
    }
    if let Some(plugin) = &filter.plugin
        && entry
            .fields
            .get("plugin")
            .and_then(serde_json::Value::as_str)
            != Some(plugin.as_str())
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        (dir, paths)
    }

    fn write_fixture_logs(paths: &SmabarPaths) {
        let dir = paths.logs_dir();
        fs::create_dir_all(&dir).expect("create logs dir");
        fs::write(
            dir.join("smabar.log.2026-08-20"),
            concat!(
                r#"{"timestamp":"2026-08-20T10:00:00Z","level":"INFO","fields":{"message":"boot"},"target":"smabar_core::app"}"#,
                "\n",
                "garbage that is not json\n",
                r#"{"timestamp":"2026-08-20T11:00:00Z","level":"WARN","fields":{"message":"low battery","percent":9},"target":"smabar_core::power"}"#,
                "\n",
            ),
        )
        .expect("write log file");
        fs::write(
            dir.join("smabar.log.2026-08-21"),
            concat!(
                r#"{"timestamp":"2026-08-21T09:00:00Z","level":"DEBUG","fields":{"message":"tick"},"target":"smabar_core::providers"}"#,
                "\n",
                r#"{"timestamp":"2026-08-21T09:01:00Z","level":"ERROR","fields":{"message":"plugin crashed"},"target":"smabar_core::plugins"}"#,
                "\n",
                r#"{"level":"INFO"}"#,
                "\n",
            ),
        )
        .expect("write log file");
    }

    fn messages(result: &LogQueryResult) -> Vec<&str> {
        result.entries.iter().map(|e| e.message.as_str()).collect()
    }

    #[test]
    fn level_parsing_and_ordering() {
        assert_eq!(LogLevel::parse("WARN"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("nope"), None);
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn query_returns_all_entries_in_order_and_counts_bad_lines() {
        let (_dir, paths) = temp_paths();
        write_fixture_logs(&paths);

        let result = query(&paths, &LogFilter::default()).expect("query");
        assert_eq!(
            messages(&result),
            vec!["boot", "low battery", "tick", "plugin crashed"]
        );
        // "garbage" line + the line missing timestamp/target.
        assert_eq!(result.skipped_lines, 2);

        let battery = &result.entries[1];
        assert_eq!(battery.level, LogLevel::Warn);
        assert_eq!(battery.target, "smabar_core::power");
        assert_eq!(battery.timestamp, "2026-08-20T11:00:00Z");
        assert_eq!(battery.fields, serde_json::json!({ "percent": 9 }));
    }

    #[test]
    fn query_filters_by_min_level_substring_and_limit() {
        let (_dir, paths) = temp_paths();
        write_fixture_logs(&paths);

        let warnings = query(
            &paths,
            &LogFilter {
                min_level: Some(LogLevel::Warn),
                ..LogFilter::default()
            },
        )
        .expect("query");
        assert_eq!(messages(&warnings), vec!["low battery", "plugin crashed"]);

        // Substring matches the raw line, so field values match too.
        let by_field = query(
            &paths,
            &LogFilter {
                substring: Some("percent".to_string()),
                ..LogFilter::default()
            },
        )
        .expect("query");
        assert_eq!(messages(&by_field), vec!["low battery"]);

        let newest_two = query(
            &paths,
            &LogFilter {
                limit: Some(2),
                ..LogFilter::default()
            },
        )
        .expect("query");
        assert_eq!(messages(&newest_two), vec!["tick", "plugin crashed"]);

        let combined = query(
            &paths,
            &LogFilter {
                min_level: Some(LogLevel::Info),
                limit: Some(1),
                ..LogFilter::default()
            },
        )
        .expect("query");
        assert_eq!(messages(&combined), vec!["plugin crashed"]);
    }

    #[test]
    fn query_on_missing_logs_dir_is_empty_not_an_error() {
        let (_dir, paths) = temp_paths();
        let result = query(&paths, &LogFilter::default()).expect("query");
        assert!(result.entries.is_empty());
        assert_eq!(result.skipped_lines, 0);
    }

    #[test]
    fn init_writes_queryable_jsonl_and_tolerates_double_init() {
        let (_dir, paths) = temp_paths();
        let first = super::super::init(&paths).expect("first init");
        let second = super::super::init(&paths).expect("second init must not panic");
        drop(second);

        let marker = format!("init-roundtrip-marker-{}", std::process::id());
        tracing::info!(marker = %marker, "logging init roundtrip");
        drop(first); // flushes the non-blocking writer

        // The flush happens on a background thread; poll briefly.
        let filter = LogFilter {
            substring: Some(marker.clone()),
            ..LogFilter::default()
        };
        let mut found = Vec::new();
        for _ in 0..40 {
            found = query(&paths, &filter).expect("query").entries;
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(found.len(), 1, "expected exactly one marker entry");
        assert_eq!(found[0].level, LogLevel::Info);
        assert_eq!(found[0].message, "logging init roundtrip");
        assert_eq!(found[0].fields, serde_json::json!({ "marker": marker }));
    }
}
