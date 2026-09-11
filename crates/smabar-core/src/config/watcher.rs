//! Hot-reload watcher for the config file: debounce plus write-echo suppression.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::util::lock_unpoisoned;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::{broadcast, mpsc};

use super::model::write_atomic;
use super::{ConfigError, SmabarConfig, SmabarPaths};

const DEBOUNCE: Duration = Duration::from_millis(500);
const CHANNEL_CAPACITY: usize = 16;

/// A confirmed change of the on-disk config.
///
/// Consumers classify the change themselves via the `*_changed` helpers.
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub old: SmabarConfig,
    pub new: SmabarConfig,
}

impl ConfigChange {
    /// Did the bar layout (position, variant, zone sizing) change?
    pub fn layout_changed(&self) -> bool {
        self.old.layout != self.new.layout
    }

    /// Did the pinned shortcuts or their labels setting change?
    pub fn shortcuts_changed(&self) -> bool {
        self.old.shortcuts != self.new.shortcuts
    }

    /// Did the set of disabled (hidden) tiles change?
    pub fn plugins_hidden_changed(&self) -> bool {
        self.old.plugins_hidden != self.new.plugins_hidden
    }

    /// Did the set of deactivated plugins change? This is what makes the
    /// supervisor stop or start a plugin process.
    pub fn plugins_deactivated_changed(&self) -> bool {
        self.old.plugins_deactivated != self.new.plugins_deactivated
    }

    /// Did the visual effects settings change?
    pub fn effects_changed(&self) -> bool {
        self.old.effects != self.new.effects
    }

    /// Did the UI language change?
    pub fn language_changed(&self) -> bool {
        self.old.language != self.new.language
    }

    /// Did the active theme change?
    pub fn theme_changed(&self) -> bool {
        self.old.theme != self.new.theme
    }

    /// Did the persisted settings-panel dimensions change?
    pub fn settings_window_changed(&self) -> bool {
        self.old.settings_window != self.new.settings_window
    }
}

/// Pure change-detection state: current config plus the hash of the last
/// file content we consider "ours" (loaded or written by us).
#[derive(Debug)]
struct WatchState {
    current: SmabarConfig,
    last_hash: u64,
}

impl WatchState {
    fn new(current: SmabarConfig, content: &str) -> Self {
        Self {
            current,
            last_hash: content_hash(content),
        }
    }

    /// Classify freshly read file content. Returns the change to broadcast,
    /// if any. Write echoes (unchanged hash), formatting-only edits, and
    /// invalid JSON all yield `None`; invalid JSON keeps the previous config.
    fn observe(&mut self, content: &str) -> Option<ConfigChange> {
        let hash = content_hash(content);
        if hash == self.last_hash {
            return None;
        }
        let new: SmabarConfig = match serde_json::from_str(content) {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(%error, "config file changed to invalid JSON; keeping previous config");
                return None;
            }
        };
        if let Err(error) = new.validate() {
            tracing::warn!(%error, "config file changed to invalid values; keeping previous config");
            return None;
        }
        self.last_hash = hash;
        if new == self.current {
            return None;
        }
        let old = std::mem::replace(&mut self.current, new.clone());
        Some(ConfigChange { old, new })
    }

    /// Record our own write so the resulting filesystem event is suppressed.
    fn record_write(&mut self, content: &str, config: SmabarConfig) {
        self.last_hash = content_hash(content);
        self.current = config;
    }
}

fn content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Watches the config file and broadcasts [`ConfigChange`]s for real external
/// edits (~500 ms debounce). Writes done through [`ConfigWatcher::save`] do
/// not trigger a reload event.
pub struct ConfigWatcher {
    paths: SmabarPaths,
    state: Arc<Mutex<WatchState>>,
    changes: broadcast::Sender<ConfigChange>,
    /// Dropping this stops the OS-level watch and, transitively, the
    /// debounce task (its event channel closes).
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Load the config (creating defaults if missing) and start watching the
    /// file for external edits. Must be called inside a tokio runtime.
    pub fn spawn(paths: SmabarPaths) -> Result<Self, ConfigError> {
        Self::spawn_with_debounce(paths, DEBOUNCE)
    }

    fn spawn_with_debounce(paths: SmabarPaths, debounce: Duration) -> Result<Self, ConfigError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| ConfigError::NoTokioRuntime)?;
        let initial = SmabarConfig::load(&paths)?;
        let config_file = paths.config_file();
        let content = std::fs::read_to_string(&config_file).map_err(|source| ConfigError::Io {
            path: config_file.clone(),
            source,
        })?;
        let state = Arc::new(Mutex::new(WatchState::new(initial, &content)));

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (changes, _) = broadcast::channel(CHANNEL_CAPACITY);

        // Watch the base directory, not the file: editors replace the file via
        // rename, which would silently detach a watch on the file itself.
        let mut watcher =
            notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                match result {
                    Ok(event) => {
                        let concerns_config = !event.kind.is_access()
                            && event
                                .paths
                                .iter()
                                .any(|p| p.file_name() == config_file.file_name());
                        if concerns_config {
                            // Send fails only when the debounce task is gone,
                            // i.e. the watcher itself is being dropped.
                            let _ = event_tx.send(());
                        }
                    }
                    Err(error) => tracing::warn!(%error, "config file watcher reported an error"),
                }
            })?;
        watcher.watch(paths.base_dir(), RecursiveMode::NonRecursive)?;

        runtime.spawn(debounce_loop(
            event_rx,
            Arc::clone(&state),
            paths.clone(),
            changes.clone(),
            debounce,
        ));

        Ok(Self {
            paths,
            state,
            changes,
            _watcher: watcher,
        })
    }

    /// The most recently loaded or saved config.
    pub fn current(&self) -> SmabarConfig {
        lock_unpoisoned(&self.state).current.clone()
    }

    /// Reads the current snapshot while preventing a concurrent update.
    /// Keep the callback short; this exists for side effects whose order must
    /// match config mutations, such as emitting an active-theme refresh.
    pub fn with_current<T>(&self, read: impl FnOnce(&SmabarConfig) -> T) -> T {
        read(&lock_unpoisoned(&self.state).current)
    }

    /// Subscribe to confirmed external config changes.
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChange> {
        self.changes.subscribe()
    }

    /// Computes and persists one relative mutation while holding the current
    /// config snapshot stable. Use this instead of `current()` followed by
    /// `apply()`, which can overwrite an unrelated concurrent mutation.
    pub fn update<T>(
        &self,
        update: impl FnOnce(&SmabarConfig) -> (SmabarConfig, T),
    ) -> Result<T, ConfigError> {
        let output = {
            let mut state = lock_unpoisoned(&self.state);
            let (new, output) = update(&state.current);
            if state.current == new {
                return Ok(output);
            }
            let json = new.to_pretty_json()?;
            write_atomic(&self.paths, &json)?;
            let old = state.current.clone();
            state.record_write(&json, new.clone());
            if self.changes.send(ConfigChange { old, new }).is_err() {
                tracing::debug!("applied config change had no subscribers");
            }
            output
        };
        Ok(output)
    }

    /// Atomically save `config` without triggering a reload event (write-echo
    /// suppression).
    pub fn save(&self, config: &SmabarConfig) -> Result<(), ConfigError> {
        let json = config.to_pretty_json()?;
        let mut state = lock_unpoisoned(&self.state);
        write_atomic(&self.paths, &json)?;
        state.record_write(&json, config.clone());
        Ok(())
    }

    /// Like [`ConfigWatcher::save`], but also broadcasts the resulting
    /// [`ConfigChange`] to subscribers — the write path for programmatic
    /// mutations (MCP tools), so live consumers react as if the file had been
    /// edited externally. An unchanged config writes nothing and emits nothing.
    pub fn apply(&self, new: SmabarConfig) -> Result<(), ConfigError> {
        self.update(move |_| (new, ()))
    }
}

async fn debounce_loop(
    mut events: mpsc::UnboundedReceiver<()>,
    state: Arc<Mutex<WatchState>>,
    paths: SmabarPaths,
    changes: broadcast::Sender<ConfigChange>,
    debounce: Duration,
) {
    while events.recv().await.is_some() {
        // Absorb the burst: wait until the file has been quiet for `debounce`.
        loop {
            match tokio::time::timeout(debounce, events.recv()).await {
                Ok(Some(())) => {}
                Ok(None) => return,
                Err(_) => break,
            }
        }
        let config_file = paths.config_file();
        // Read and apply under the same lock as programmatic writes. Reading
        // first would let an older external snapshot overwrite a newer
        // `update()` that won the lock while this task was opening the file.
        let change = {
            let mut state = lock_unpoisoned(&state);
            let content = match std::fs::read_to_string(&config_file) {
                Ok(content) => content,
                Err(error) => {
                    tracing::warn!(%error, path = %config_file.display(), "cannot read changed config file");
                    continue;
                }
            };
            let change = state.observe(&content);
            if let Some(change) = &change
                && changes.send(change.clone()).is_err()
            {
                tracing::debug!("config change had no subscribers");
            }
            change
        };
        if let Some(change) = change {
            tracing::info!(
                layout_changed = change.layout_changed(),
                language_changed = change.language_changed(),
                "config file changed on disk; reloaded"
            );
        }
    }
}

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod tests;
