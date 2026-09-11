//! Waiting for the outcome of a plugin (re)start.
//!
//! Writing a plugin file triggers a reload asynchronously, so a caller that
//! wants to report the real result has to watch the event bus. Two steps on
//! purpose: a [`broadcast::Receiver`] only sees events sent after `subscribe`,
//! and the reload is triggered by the caller's own write — so the caller must
//! [`PluginSupervisor::watch_status`](super::PluginSupervisor::watch_status)
//! FIRST, then act, then [`StatusWatcher::settle`].

use std::time::Duration;

use tokio::sync::broadcast;

use super::{PluginEvent, PluginStatus};

/// How a (re)start ended.
#[derive(Debug, Clone)]
pub struct ReloadOutcome {
    pub status: PluginStatus,
    /// Failure reason, set when `status` is [`PluginStatus::Failed`].
    pub error: Option<String>,
    /// Whether a terminal event actually arrived. `false` means the wait timed
    /// out and `status` is the last known level, not a settled result — a
    /// plugin's first python run provisions a toolchain and can take minutes.
    pub settled: bool,
}

/// Subscribed to the plugin event bus, waiting for one plugin's next outcome.
pub struct StatusWatcher {
    rx: broadcast::Receiver<PluginEvent>,
    plugin_id: String,
}

impl StatusWatcher {
    pub(super) fn new(rx: broadcast::Receiver<PluginEvent>, plugin_id: &str) -> Self {
        Self {
            rx,
            plugin_id: plugin_id.to_string(),
        }
    }

    /// Waits for this plugin to reach a terminal state, at most `timeout`.
    ///
    /// `fallback` is the level to report when nothing terminal arrives (the
    /// caller's current [`plugin_infos`](super::PluginSupervisor::plugin_infos)
    /// entry): events give the edge, the status map gives the level.
    pub async fn settle(mut self, timeout: Duration, fallback: PluginStatus) -> ReloadOutcome {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let event = match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                Ok(Ok(event)) => event,
                // Lagged: UiRender shares this bus, so a busy bar can push the
                // status event out of the buffer. Keep waiting — the fallback
                // covers us if it was the one we lost.
                Ok(Err(broadcast::error::RecvError::Lagged(skipped))) => {
                    tracing::debug!(skipped, plugin = %self.plugin_id, "status watch lagged");
                    continue;
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
                Err(_) => break,
            };
            match event {
                PluginEvent::Status {
                    plugin_id,
                    status,
                    error,
                } if plugin_id == self.plugin_id => match status {
                    // Deactivated is as final as running or failed: the
                    // plugin was deliberately not started, and waiting out
                    // the timeout would only hide that answer.
                    PluginStatus::Running | PluginStatus::Failed | PluginStatus::Deactivated => {
                        return ReloadOutcome {
                            status,
                            error,
                            settled: true,
                        };
                    }
                    // A reload stops the OLD instance before starting the new
                    // one, so Stopped/Starting are mid-flight, not results.
                    PluginStatus::Starting | PluginStatus::Stopped => continue,
                },
                PluginEvent::Removed { plugin_id } if plugin_id == self.plugin_id => {
                    return ReloadOutcome {
                        status: PluginStatus::Stopped,
                        error: None,
                        settled: true,
                    };
                }
                _ => continue,
            }
        }
        ReloadOutcome {
            status: fallback,
            error: None,
            settled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(id: &str, status: PluginStatus) -> PluginEvent {
        PluginEvent::Status {
            plugin_id: id.to_string(),
            status,
            error: None,
        }
    }

    #[tokio::test]
    async fn running_after_a_stop_settles_as_running() {
        let (tx, _) = broadcast::channel(8);
        let watcher = StatusWatcher::new(tx.subscribe(), "demo");
        tx.send(status("demo", PluginStatus::Stopped))
            .expect("send");
        tx.send(status("demo", PluginStatus::Starting))
            .expect("send");
        tx.send(status("demo", PluginStatus::Running))
            .expect("send");

        let outcome = watcher
            .settle(Duration::from_secs(5), PluginStatus::Starting)
            .await;
        assert_eq!(outcome.status, PluginStatus::Running);
        assert!(outcome.settled);
    }

    #[tokio::test]
    async fn the_first_failure_settles_with_its_reason() {
        let (tx, _) = broadcast::channel(8);
        let watcher = StatusWatcher::new(tx.subscribe(), "demo");
        tx.send(PluginEvent::Status {
            plugin_id: "demo".to_string(),
            status: PluginStatus::Failed,
            error: Some("boom".to_string()),
        })
        .expect("send");

        let outcome = watcher
            .settle(Duration::from_secs(5), PluginStatus::Starting)
            .await;
        assert_eq!(outcome.status, PluginStatus::Failed);
        assert_eq!(outcome.error.as_deref(), Some("boom"));
        assert!(outcome.settled);
    }

    #[tokio::test]
    async fn other_plugins_are_ignored_and_a_timeout_reports_the_fallback() {
        let (tx, _) = broadcast::channel(8);
        let watcher = StatusWatcher::new(tx.subscribe(), "demo");
        tx.send(status("other", PluginStatus::Running))
            .expect("send");

        let outcome = watcher
            .settle(Duration::from_millis(50), PluginStatus::Starting)
            .await;
        assert_eq!(outcome.status, PluginStatus::Starting);
        assert!(!outcome.settled);
    }
}
