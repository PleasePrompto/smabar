//! Persisted source classification and pure pin-list mutations.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{ShortcutEntry, SmabarConfig, SpecialShortcut};

use super::ShortcutError;

#[derive(Debug, Clone, Copy)]
pub(super) enum EntrySource<'a> {
    DesktopId(&'a str),
    Path(&'a Path),
    Url(&'a str),
    Special(SpecialShortcut),
    Separator,
}

/// Applies the exactly-one-source invariant to both new and persisted pins.
pub(super) fn entry_source(entry: &ShortcutEntry) -> Result<EntrySource<'_>, ShortcutError> {
    let source_count = usize::from(entry.desktop_id.is_some())
        + usize::from(entry.path.is_some())
        + usize::from(entry.url.is_some())
        + usize::from(entry.special.is_some());
    if entry.separator {
        return (source_count == 0)
            .then_some(EntrySource::Separator)
            .ok_or(ShortcutError::InvalidSource);
    }
    if source_count != 1 {
        return Err(ShortcutError::InvalidSource);
    }
    if let Some(id) = &entry.desktop_id {
        return Ok(EntrySource::DesktopId(id));
    }
    if let Some(path) = &entry.path {
        return Ok(EntrySource::Path(path));
    }
    if let Some(url) = &entry.url {
        return Ok(EntrySource::Url(url));
    }
    entry
        .special
        .map(EntrySource::Special)
        .ok_or(ShortcutError::InvalidSource)
}

/// Pure insertion with duplicate-source protection.
pub fn pin_shortcut(
    config: &SmabarConfig,
    entry: ShortcutEntry,
    index: Option<usize>,
) -> Result<SmabarConfig, ShortcutError> {
    if config.shortcuts.pinned.iter().any(|pin| pin.id == entry.id) {
        return Err(ShortcutError::AlreadyPinned { id: entry.id });
    }
    let mut new = config.clone();
    let len = new.shortcuts.pinned.len();
    new.shortcuts
        .pinned
        .insert(index.unwrap_or(len).min(len), entry);
    Ok(new)
}

/// Pure removal by generated pin id.
pub fn unpin_shortcut(config: &SmabarConfig, id: &str) -> Result<SmabarConfig, ShortcutError> {
    let mut new = config.clone();
    let before = new.shortcuts.pinned.len();
    new.shortcuts.pinned.retain(|pin| pin.id != id);
    if new.shortcuts.pinned.len() == before {
        return Err(ShortcutError::UnknownPin { id: id.to_string() });
    }
    Ok(new)
}

pub(super) fn pin_id(source: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let hex = format!("{:016x}", hasher.finish());
    format!("sc-{}", &hex[..8])
}

pub(super) fn separator_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "sc-separator-{timestamp:x}-{:x}-{sequence:x}",
        std::process::id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_entries_also_require_exactly_one_source() {
        let mut entry = ShortcutEntry {
            id: "sc-test".to_string(),
            desktop_id: Some("app.desktop".to_string()),
            ..ShortcutEntry::default()
        };
        assert!(matches!(
            entry_source(&entry),
            Ok(EntrySource::DesktopId(_))
        ));
        entry.url = Some("https://example.com".to_string());
        assert!(matches!(
            entry_source(&entry),
            Err(ShortcutError::InvalidSource)
        ));
        entry.desktop_id = None;
        entry.url = None;
        entry.special = Some(SpecialShortcut::Trash);
        assert!(matches!(entry_source(&entry), Ok(EntrySource::Special(_))));
        entry.separator = true;
        assert!(matches!(
            entry_source(&entry),
            Err(ShortcutError::InvalidSource)
        ));
    }
}
