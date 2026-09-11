//! The one stderr line that explains a crashed start.
//!
//! A python plugin that dies before its first RPC line leaves a traceback on
//! stderr and a generic "plugin closed stdout" as its failure reason. The
//! traceback's LAST line (`ModuleNotFoundError: No module named 'views'`) is
//! what an agent has to read; burying it under twenty log entries is how the
//! same missing file got written three times.

/// True for a line that names an error, the way Python, uv and pip end their
/// output: `SomeError: …`, `some.module.SomeException: …`, `error: …`.
/// Traceback frames, progress lines and warnings are not causes.
pub(crate) fn is_cause_line(line: &str) -> bool {
    let Some((head, tail)) = line.trim().split_once(':') else {
        return false;
    };
    if head.is_empty()
        || tail.trim().is_empty()
        || !head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return false;
    }
    head == "error"
        || head == "SystemExit"
        || head.ends_with("Error")
        || head.ends_with("Exception")
}

/// The last cause line among `lines`, in order of appearance.
pub(crate) fn last_cause<'a>(lines: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    lines
        .filter(|line| is_cause_line(line))
        .last()
        .map(str::trim)
}

/// Remembers the last cause line of one process run.
#[derive(Debug, Default)]
pub(crate) struct StderrCause {
    last: Option<String>,
}

impl StderrCause {
    pub(crate) fn observe(&mut self, line: &str) {
        if is_cause_line(line) {
            self.last = Some(line.trim().to_string());
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last = None;
    }

    pub(crate) fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }

    /// Puts the cause in front of a generic failure reason. Specific reasons
    /// (a ping timeout, a missing runtime) already say what happened.
    pub(crate) fn lift(&self, reason: String) -> String {
        match self.last() {
            Some(cause)
                if reason.starts_with("plugin closed stdout")
                    || reason.starts_with("initialize failed") =>
            {
                format!("{cause} — {reason}")
            }
            _ => reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACEBACK: &str = "Traceback (most recent call last):
  File \"/home/u/.smabar/plugins/demo/plugin.py\", line 3, in <module>
    import views
ModuleNotFoundError: No module named 'views'";

    #[test]
    fn the_last_line_of_a_traceback_is_the_cause() {
        assert_eq!(
            last_cause(TRACEBACK.lines()),
            Some("ModuleNotFoundError: No module named 'views'")
        );
        assert!(is_cause_line(
            "smabar_sdk.protocol.RpcError: initialize rejected"
        ));
        assert!(is_cause_line("error: Failed to spawn: `python3`"));
        assert!(is_cause_line("SystemExit: 2"));
    }

    #[test]
    fn frames_progress_and_warnings_are_not_causes() {
        for line in [
            "Traceback (most recent call last):",
            "  File \"plugin.py\", line 3, in <module>",
            "Resolved 3 packages in 120ms",
            "Installed 3 packages in 40ms",
            "warning: The package `x` does not have an extra",
            "http://localhost:8080",
            "Error:",
        ] {
            assert!(!is_cause_line(line), "{line:?} must not count as a cause");
        }
        assert_eq!(
            last_cause(["Resolved 1 package", "Installed 1 package"].into_iter()),
            None
        );
    }

    #[test]
    fn only_generic_reasons_get_the_cause_in_front() {
        let mut cause = StderrCause::default();
        for line in TRACEBACK.lines() {
            cause.observe(line);
        }
        assert_eq!(
            cause.lift("plugin closed stdout (process exit or crash)".to_string()),
            "ModuleNotFoundError: No module named 'views' — plugin closed stdout (process exit or crash)"
        );
        assert_eq!(
            cause.lift("ping failed: timed out".to_string()),
            "ping failed: timed out"
        );
        cause.reset();
        assert_eq!(
            cause.lift("initialize failed: closed".to_string()),
            "initialize failed: closed"
        );
    }
}
