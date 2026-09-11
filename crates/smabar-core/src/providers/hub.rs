use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::{broadcast, watch};
use tokio::time::MissedTickBehavior;

use super::config::{AudioAction, MediaAction, ProviderConfig, ProviderKind};
use crate::util::now_ms;

use super::data::ProviderEvent;
use crate::util::lock_unpoisoned;

use super::sampler::{NetworkTotals, SystemSampler};
use crate::platform::{AudioError, MediaError};

/// Broadcast backlog per subscriber; slow consumers skip to the newest events.
const EVENT_CHANNEL_CAPACITY: usize = 64;

/// One running sampler task, shared by all subscribers of the same config.
struct SamplerEntry {
    subscriber_count: usize,
    stop: watch::Sender<()>,
}

struct HubInner {
    event_tx: broadcast::Sender<Arc<ProviderEvent>>,
    samplers: Mutex<HashMap<ProviderConfig, SamplerEntry>>,
    /// Last emitted event per config, replayed to new subscribers. Kept for
    /// the hub's lifetime, so a re-subscribe shows the last known value
    /// immediately (the restarted sampler's first tick refreshes it).
    cache: Arc<Mutex<HashMap<ProviderConfig, ProviderEvent>>>,
    sampler: SystemSampler,
}

impl HubInner {
    fn unsubscribe(&self, config: &ProviderConfig) {
        let mut samplers = lock_unpoisoned(&self.samplers);
        let Some(entry) = samplers.get_mut(config) else {
            return;
        };
        entry.subscriber_count -= 1;
        if entry.subscriber_count == 0 {
            if let Some(entry) = samplers.remove(config) {
                let _ = entry.stop.send(());
            }
            tracing::debug!(provider = %config.key(), "last subscriber dropped; sampler stopped");
        }
    }
}

/// Refcounting fan-out hub for system samplers.
///
/// One tokio task runs per distinct normalized [`ProviderConfig`]; further
/// subscriptions to the same config reuse it. All events flow through one
/// shared broadcast channel and are keyed by [`ProviderEvent::key`].
/// Cloning the hub is cheap and shares all state.
#[derive(Clone)]
pub struct ProviderHub {
    inner: Arc<HubInner>,
}

impl ProviderHub {
    /// Creates a hub. Enumerates disks and network interfaces once, so call
    /// this during startup, not on a latency-sensitive path.
    pub fn new() -> Self {
        Self::with_sampler(SystemSampler::new())
    }

    fn with_sampler(sampler: SystemSampler) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            inner: Arc::new(HubInner {
                event_tx,
                samplers: Mutex::new(HashMap::new()),
                cache: Arc::new(Mutex::new(HashMap::new())),
                sampler,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_sampler(sampler: SystemSampler) -> Self {
        Self::with_sampler(sampler)
    }

    /// Subscribes to the provider described by `config` (normalized first).
    ///
    /// Reuses a running sampler for an identical config; otherwise starts one
    /// whose first sample happens immediately. When a cached value exists it
    /// is replayed as the subscription's first event.
    pub async fn subscribe(&self, config: ProviderConfig) -> Subscription {
        let config = config.normalized();
        // Receiver first: an emission between cache read and receiver
        // creation must not be lost (a duplicate after replay is fine).
        let receiver = self.inner.event_tx.subscribe();
        let replay = lock_unpoisoned(&self.inner.cache).get(&config).cloned();

        let mut samplers = lock_unpoisoned(&self.inner.samplers);
        match samplers.get_mut(&config) {
            Some(entry) => entry.subscriber_count += 1,
            None => {
                tracing::debug!(provider = %config.key(), "starting sampler");
                let entry = spawn_sampler(&self.inner, config);
                samplers.insert(config, entry);
            }
        }
        drop(samplers);

        Subscription {
            key: config.key(),
            config,
            replay,
            receiver,
            hub: Arc::clone(&self.inner),
        }
    }

    /// Number of currently running sampler tasks (one per distinct config).
    pub fn active_sampler_count(&self) -> usize {
        lock_unpoisoned(&self.inner.samplers).len()
    }

    /// Provider names available on this target, in stable advertisement order.
    pub fn available_names(&self) -> Vec<&'static str> {
        ProviderKind::ALL
            .into_iter()
            .filter(|kind| self.inner.sampler.supports(*kind))
            .map(ProviderKind::as_str)
            .collect()
    }

    pub(crate) fn supports(&self, kind: ProviderKind) -> bool {
        self.inner.sampler.supports(kind)
    }

    pub(crate) async fn media_action(
        &self,
        session_id: Option<&str>,
        action: MediaAction,
    ) -> Result<(), MediaError> {
        self.inner.sampler.media_action(session_id, action).await
    }

    pub(crate) async fn audio_action(&self, action: AudioAction) -> Result<(), AudioError> {
        self.inner.sampler.audio_action(action).await
    }

    /// Last cached event of every provider that has been subscribed at least
    /// once, sorted by key for deterministic output.
    pub fn snapshot(&self) -> Vec<ProviderEvent> {
        let mut events: Vec<ProviderEvent> = lock_unpoisoned(&self.inner.cache)
            .values()
            .cloned()
            .collect();
        events.sort_by(|a, b| a.key.cmp(&b.key));
        events
    }

    /// Seeds the replay cache without running a sampler.
    #[cfg(test)]
    pub(crate) fn inject_cache(&self, config: ProviderConfig, event: ProviderEvent) {
        lock_unpoisoned(&self.inner.cache).insert(config.normalized(), event);
    }
}

impl Default for ProviderHub {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_sampler(inner: &Arc<HubInner>, config: ProviderConfig) -> SamplerEntry {
    let (stop_tx, stop_rx) = watch::channel(());
    let sampler = inner.sampler.clone();
    let event_tx = inner.event_tx.clone();
    let cache = Arc::clone(&inner.cache);
    tokio::spawn(run_sampler(config, sampler, event_tx, cache, stop_rx));
    SamplerEntry {
        subscriber_count: 1,
        stop: stop_tx,
    }
}

async fn run_sampler(
    config: ProviderConfig,
    sampler: SystemSampler,
    event_tx: broadcast::Sender<Arc<ProviderEvent>>,
    cache: Arc<Mutex<HashMap<ProviderConfig, ProviderEvent>>>,
    mut stop_rx: watch::Receiver<()>,
) {
    let key = config.key();
    let mut interval = tokio::time::interval(config.interval());
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut network_baseline: Option<NetworkTotals> = None;
    let mut previous_data: Option<Value> = None;
    let mut last_failure: Option<String> = None;

    loop {
        tokio::select! {
            _ = stop_rx.changed() => break,
            _ = interval.tick() => {
                let sampled = sampler.sample(config.kind, &mut network_baseline).await;
                // The stop signal may have arrived during the sample; don't
                // emit or cache on behalf of a deregistered sampler.
                if stop_rx.has_changed().unwrap_or(true) {
                    break;
                }
                match sampled {
                    Ok(data) => {
                        should_log_failure(&mut last_failure, None);
                        // Unchanged payloads are not re-emitted; subscribers
                        // only see value changes (plus the initial replay).
                        if previous_data.as_ref() == Some(&data) {
                            continue;
                        }
                        let event = ProviderEvent {
                            key: key.clone(),
                            kind: config.kind,
                            data: data.clone(),
                            ts_ms: now_ms(),
                        };
                        previous_data = Some(data);
                        lock_unpoisoned(&cache).insert(config, event.clone());
                        // Err means no active receiver right now; the cache
                        // still replays the value to the next subscriber.
                        let _ = event_tx.send(Arc::new(event));
                    }
                    Err(error) => {
                        let error = error.to_string();
                        if should_log_failure(&mut last_failure, Some(&error)) {
                            tracing::warn!(provider = %key, %error, "sampling failed; tick skipped");
                        }
                    }
                }
            }
        }
    }
}

/// True only for a new consecutive failure. A successful sample clears the
/// remembered value so the same error is visible if the provider later fails
/// again after recovering.
fn should_log_failure(last: &mut Option<String>, failure: Option<&str>) -> bool {
    let Some(failure) = failure else {
        *last = None;
        return false;
    };
    if last.as_deref() == Some(failure) {
        return false;
    }
    *last = Some(failure.to_string());
    true
}

/// Live subscription to one provider's event stream.
///
/// Dropping the last subscription of a config stops its sampler task.
pub struct Subscription {
    key: String,
    config: ProviderConfig,
    replay: Option<ProviderEvent>,
    receiver: broadcast::Receiver<Arc<ProviderEvent>>,
    hub: Arc<HubInner>,
}

impl Subscription {
    /// Stable event key of the subscribed provider: `"<kind>:<interval_ms>"`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Receives the next event for this provider.
    ///
    /// The first call returns the cached last value immediately when one
    /// exists (last-value replay). A subscriber that falls more than the
    /// channel backlog behind skips ahead to the newest events. Returns
    /// `None` only when the event channel is closed.
    pub async fn recv(&mut self) -> Option<ProviderEvent> {
        if let Some(event) = self.replay.take() {
            return Some(event);
        }
        loop {
            match self.receiver.recv().await {
                Ok(event) if event.key == self.key => return Some((*event).clone()),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::debug!(provider = %self.key, skipped, "subscriber lagged; skipping to newest events");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.unsubscribe(&self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::should_log_failure;

    #[test]
    fn identical_sampler_failures_log_once_until_recovery() {
        let mut last_failure = None;

        assert!(should_log_failure(
            &mut last_failure,
            Some("session bus offline")
        ));
        assert!(!should_log_failure(
            &mut last_failure,
            Some("session bus offline")
        ));
        assert!(should_log_failure(
            &mut last_failure,
            Some("player timed out")
        ));
        assert!(!should_log_failure(&mut last_failure, None));
        assert!(should_log_failure(
            &mut last_failure,
            Some("player timed out")
        ));
    }
}
