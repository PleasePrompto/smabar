//! Per-plugin JSONL log file: `logs/plugin-<id>.log`.
//!
//! Two writers: [`PluginLog`], held open by the plugin's own supervisor task,
//! and [`append_plugin_log`] for everything ELSE that has something to say
//! about a plugin — today the shell, reporting the markup its sanitizer had
//! to drop. Both append single lines to a file opened with `O_APPEND`, which
//! the kernel serializes, so they never interleave mid-line.

use crate::util::lock_unpoisoned;
use crate::util::now_ms;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

/// Every `source` a plugin log entry can carry, in the order an author
/// meets them: the plugin's own log calls, its printed lines, its
/// tracebacks, the core's rejections, the shell's markup reports. The tool
/// description, the guide and the entry type all describe exactly this list;
/// a test keeps them in step.
pub const LOG_SOURCES: [&str; 5] = ["log", "stdout", "stderr", "core", "shell"];

/// One JSONL line in a plugin log file.
#[derive(Serialize)]
struct LogLine<'a> {
    /// Unix timestamp in milliseconds.
    ts: u64,
    level: &'a str,
    /// One of [`LOG_SOURCES`].
    source: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<&'a Value>,
}

/// A plugin log is rotated once it passes this size.
///
/// Generous on purpose: a busy plugin writes tens of KiB in days, so the cap
/// only ever catches a runaway loop — the case where an unbounded file would
/// otherwise fill the disk and bury every useful line.
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const MAX_CORE_DIAGNOSTICS_PER_PLUGIN: usize = 128;
const DIAGNOSTIC_LIMIT_MESSAGE: &str = "more than 128 distinct malformed-input diagnostics were produced; further diagnostics are suppressed for this plugin until smabar restarts. Fix the plugin's input loop using plugin_guide.";

/// Synchronous append writer for one plugin's JSONL log.
pub(crate) struct PluginLog {
    plugin_id: String,
    file: Option<File>,
    /// The last error line stderr produced in the current run.
    cause: super::stderr_cause::StderrCause,
}

/// Deduplicates only core diagnostics about malformed plugin input.
/// Ordinary plugin, stdout and stderr entries must remain lossless.
#[derive(Clone)]
pub(crate) struct PluginDiagnostics {
    logs_dir: PathBuf,
    histories: Arc<Mutex<HashMap<String, DiagnosticHistory>>>,
}

#[derive(Default)]
struct DiagnosticHistory {
    seen: HashSet<String>,
    limit_reported: bool,
}

impl PluginDiagnostics {
    pub(crate) fn new(logs_dir: PathBuf) -> Self {
        Self {
            logs_dir,
            histories: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn warn_once(&self, plugin_id: &str, message: &str, fields: Option<&Value>) {
        let use_limit_message = {
            let mut histories = lock_unpoisoned(&self.histories);
            let history = histories.entry(plugin_id.to_string()).or_default();
            if history.seen.contains(message) {
                return;
            }
            if history.seen.len() < MAX_CORE_DIAGNOSTICS_PER_PLUGIN {
                history.seen.insert(message.to_string());
                false
            } else if history.limit_reported {
                return;
            } else {
                history.limit_reported = true;
                true
            }
        };
        if use_limit_message {
            self.write(plugin_id, DIAGNOSTIC_LIMIT_MESSAGE, None);
        } else {
            self.write(plugin_id, message, fields);
        }
    }

    pub(crate) fn warn_new_items(
        &self,
        plugin_id: &str,
        namespace: &str,
        items: &[String],
        message: impl FnOnce(&[&str]) -> String,
    ) {
        let mut histories = lock_unpoisoned(&self.histories);
        let history = histories.entry(plugin_id.to_string()).or_default();
        let mut limit_reached = false;
        let mut new = Vec::new();
        for item in items {
            let key = format!("{namespace}:{item}");
            if history.seen.contains(&key) {
                continue;
            }
            if history.seen.len() >= MAX_CORE_DIAGNOSTICS_PER_PLUGIN {
                if !history.limit_reported {
                    history.limit_reported = true;
                    limit_reached = true;
                }
                break;
            }
            history.seen.insert(key);
            new.push(item.as_str());
        }
        drop(histories);
        if new.is_empty() && !limit_reached {
            return;
        }
        let mut message = if new.is_empty() {
            String::new()
        } else {
            message(&new)
        };
        if limit_reached {
            if !message.is_empty() {
                message.push(' ');
            }
            message.push_str(DIAGNOSTIC_LIMIT_MESSAGE);
        }
        self.write(plugin_id, &message, None);
    }

    pub(crate) fn forget(&self, plugin_id: &str) {
        lock_unpoisoned(&self.histories).remove(plugin_id);
    }

    fn write(&self, plugin_id: &str, message: &str, fields: Option<&Value>) {
        tracing::warn!(plugin = plugin_id, %message, "plugin input adjusted");
        append_plugin_log(&self.logs_dir, plugin_id, "warn", "core", message, fields);
    }
}

impl PluginLog {
    /// Opens (creating if needed) `logs_dir/plugin-<id>.log` in append mode.
    /// On failure the log degrades to tracing-only instead of erroring.
    pub(crate) fn open(logs_dir: &Path, plugin_id: &str) -> Self {
        let path = logs_dir.join(format!("plugin-{plugin_id}.log"));
        rotate_if_oversized(&path);
        let file = std::fs::create_dir_all(logs_dir)
            .and_then(|()| OpenOptions::new().create(true).append(true).open(&path))
            .map_err(|error| {
                tracing::warn!(
                    plugin = plugin_id,
                    %error,
                    path = %path.display(),
                    "cannot open plugin log file; entries go to the main log only"
                );
            })
            .ok();
        Self {
            plugin_id: plugin_id.to_string(),
            file,
            cause: super::stderr_cause::StderrCause::default(),
        }
    }

    /// Forgets the previous run's stderr cause; call it when a process spawns.
    pub(crate) fn begin_run(&mut self) {
        self.cause.reset();
    }

    /// Puts this run's stderr cause in front of a generic failure reason.
    pub(crate) fn explain_failure(&self, reason: String) -> String {
        self.cause.lift(reason)
    }

    /// Appends one entry and mirrors it into the main log at debug level.
    pub(crate) fn write(
        &mut self,
        level: &str,
        source: &str,
        message: &str,
        fields: Option<&Value>,
    ) {
        if source == "stderr" {
            self.cause.observe(message);
        }
        tracing::debug!(plugin = %self.plugin_id, level, source, message, "plugin log");
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let line = LogLine {
            ts: now_ms(),
            level,
            source,
            message,
            fields,
        };
        match serde_json::to_string(&line) {
            Ok(json) => {
                if let Err(error) = writeln!(file, "{json}") {
                    tracing::warn!(plugin = %self.plugin_id, %error, "cannot append to plugin log file");
                }
            }
            Err(error) => {
                tracing::warn!(plugin = %self.plugin_id, %error, "cannot serialize plugin log entry");
            }
        }
    }
}

/// Moves an oversized log aside so a fresh one starts.
///
/// Exactly ONE generation is kept: `plugin-<id>.log.1` is overwritten. Two
/// files bound the disk; keeping more would only postpone the same decision.
fn rotate_if_oversized(path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return; // No file yet — nothing to rotate.
    };
    if metadata.len() <= MAX_LOG_BYTES {
        return;
    }
    let previous = path.with_extension("log.1");
    match std::fs::rename(path, &previous) {
        Ok(()) => tracing::info!(
            path = %path.display(),
            bytes = metadata.len(),
            "plugin log exceeded its cap; rotated"
        ),
        Err(error) => tracing::warn!(
            path = %path.display(),
            %error,
            "cannot rotate an oversized plugin log; it keeps growing"
        ),
    }
}

/// Appends one entry to a plugin's log from outside its supervisor task.
///
/// Best effort: a plugin id that could escape the logs directory is refused,
/// and an unwritable file is reported once to the main log. Callers are
/// expected to rate-limit — the shell reports the same dropped markup on
/// every render otherwise.
pub fn append_plugin_log(
    logs_dir: &Path,
    plugin_id: &str,
    level: &str,
    source: &str,
    message: &str,
    fields: Option<&Value>,
) {
    if !crate::plugins::is_valid_plugin_id(plugin_id) {
        tracing::warn!(
            plugin = plugin_id,
            "refusing to log for an invalid plugin id"
        );
        return;
    }
    PluginLog::open(logs_dir, plugin_id).write(level, source, message, fields);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(logs_dir: &Path, plugin_id: &str) -> Vec<serde_json::Value> {
        let path = logs_dir.join(format!("plugin-{plugin_id}.log"));
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("log lines are JSON"))
            .collect()
    }

    #[test]
    fn an_outside_writer_appends_one_valid_line() {
        let dir = tempfile::tempdir().expect("temp dir");
        append_plugin_log(dir.path(), "clock", "warn", "shell", "markup removed", None);

        let written = entries(dir.path(), "clock");
        assert_eq!(written.len(), 1);
        assert_eq!(written[0]["source"], "shell");
        assert_eq!(written[0]["level"], "warn");
        assert_eq!(written[0]["message"], "markup removed");
    }

    #[test]
    fn an_id_that_could_escape_the_logs_directory_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        for bad in ["../evil", "Clock", "", "a/b"] {
            append_plugin_log(dir.path(), bad, "error", "shell", "nope", None);
        }
        let created: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read temp dir")
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert!(created.is_empty(), "nothing may be written: {created:?}");
    }

    #[test]
    fn an_oversized_log_is_rotated_into_exactly_one_generation() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("plugin-loud.log");
        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).expect("write big log");

        append_plugin_log(dir.path(), "loud", "info", "log", "after rotation", None);

        let rotated = dir.path().join("plugin-loud.log.1");
        assert!(rotated.is_file(), "one generation is kept");
        assert_eq!(
            entries(dir.path(), "loud").len(),
            1,
            "the live log starts fresh"
        );

        // A second rotation must not pile up a .2.
        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).expect("refill");
        append_plugin_log(dir.path(), "loud", "info", "log", "again", None);
        assert!(!dir.path().join("plugin-loud.log.2").exists());
        assert_eq!(entries(dir.path(), "loud").len(), 1);
    }

    #[test]
    fn a_log_below_the_cap_is_left_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        append_plugin_log(dir.path(), "quiet", "info", "log", "first", None);
        append_plugin_log(dir.path(), "quiet", "info", "log", "second", None);

        assert!(!dir.path().join("plugin-quiet.log.1").exists());
        assert_eq!(entries(dir.path(), "quiet").len(), 2);
    }

    #[test]
    fn core_diagnostics_are_once_but_ordinary_entries_remain_lossless() {
        let dir = tempfile::tempdir().expect("temp dir");
        let diagnostics = PluginDiagnostics::new(dir.path().to_path_buf());
        diagnostics.warn_once("clock", "ttlMs ignored", None);
        diagnostics.warn_once("clock", "ttlMs ignored", None);
        diagnostics.warn_once("clock", "target ignored", None);
        append_plugin_log(dir.path(), "clock", "info", "log", "same", None);
        append_plugin_log(dir.path(), "clock", "info", "log", "same", None);

        let written = entries(dir.path(), "clock");
        assert_eq!(written.len(), 4);
        assert_eq!(
            written
                .iter()
                .filter(|entry| entry["source"] == "core")
                .count(),
            2
        );
        assert_eq!(
            written
                .iter()
                .filter(|entry| entry["source"] == "log")
                .count(),
            2
        );
    }

    #[test]
    fn new_items_are_aggregated_without_repeating_old_items() {
        let dir = tempfile::tempdir().expect("temp dir");
        let diagnostics = PluginDiagnostics::new(dir.path().to_path_buf());
        let first = vec!["old".to_string()];
        let expanded = vec!["old".to_string(), "new".to_string()];
        for items in [&first, &expanded] {
            diagnostics.warn_new_items("clock", "manifest", items, |new| new.join("; "));
        }

        let written = entries(dir.path(), "clock");
        assert_eq!(written.len(), 2);
        assert_eq!(written[0]["message"], "old");
        assert_eq!(written[1]["message"], "new");
    }

    #[test]
    fn distinct_diagnostics_are_bounded_and_removal_resets_the_history() {
        let dir = tempfile::tempdir().expect("temp dir");
        let diagnostics = PluginDiagnostics::new(dir.path().to_path_buf());
        for index in 0..MAX_CORE_DIAGNOSTICS_PER_PLUGIN + 10 {
            diagnostics.warn_once("clock", &format!("problem {index}"), None);
        }

        let written = entries(dir.path(), "clock");
        assert_eq!(written.len(), MAX_CORE_DIAGNOSTICS_PER_PLUGIN + 1);
        assert_eq!(
            written.last().expect("limit entry")["message"],
            DIAGNOSTIC_LIMIT_MESSAGE
        );

        diagnostics.forget("clock");
        diagnostics.warn_once("clock", "problem 0", None);
        assert_eq!(entries(dir.path(), "clock").len(), written.len() + 1);
    }
}
