//! Tiny helpers shared across smabar-core's modules.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Locks a mutex, ignoring poisoning: the guarded state stays consistent
/// across all writers, and a panicking holder is already reported elsewhere.
pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Current Unix time in milliseconds (0 if the clock is before the epoch).
/// The timestamp currency of provider samples, plugin logs and MCP reloads.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Writes `bytes` to `target` through a sibling temp file and a rename, so a
/// reader never sees a half-written file and a crash leaves the old one.
///
/// The temp name starts with a dot: the plugin folder watcher and the
/// plugin file readers skip dot-entries.
pub(crate) fn write_atomically(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other(format!("{} has no parent directory", target.display())))?;
    fs::create_dir_all(parent)?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let temp = parent.join(format!(".{name}.tmp"));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, target).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomically_replaces_the_target_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("nested").join("state.json");
        write_atomically(&target, b"one").expect("first write");
        write_atomically(&target, b"two").expect("second write");
        assert_eq!(fs::read(&target).expect("read"), b"two");
        let leftovers: Vec<_> = fs::read_dir(target.parent().expect("parent"))
            .expect("read_dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["state.json".to_string()]);
    }
}
