//! Per-plugin transition ordering and the terminal shutdown gate.

use std::sync::{Arc, Weak};

use crate::util::lock_unpoisoned;

use super::supervisor::Inner;

/// Enters one lifecycle transition unless terminal shutdown has begun.
/// The read guard keeps `shutdown_all` waiting; the owned mutex keeps
/// transitions for the same plugin strictly ordered.
pub(super) async fn lifecycle_guard<'a>(
    inner: &'a Inner,
    plugin_id: &str,
) -> Option<(
    tokio::sync::RwLockReadGuard<'a, ()>,
    tokio::sync::OwnedMutexGuard<()>,
)> {
    let transition = inner.lifecycle_barrier.read().await;
    if inner.shutdown.is_cancelled() {
        return None;
    }
    let plugin = lifecycle_lock(inner, plugin_id).lock_owned().await;
    if inner.shutdown.is_cancelled() {
        return None;
    }
    Some((transition, plugin))
}

/// Weak entries disappear on the next lookup after the last waiter leaves.
fn lifecycle_lock(inner: &Inner, plugin_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = lock_unpoisoned(&inner.lifecycle_locks);
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(plugin_id).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(plugin_id.to_string(), Arc::downgrade(&lock));
    lock
}
