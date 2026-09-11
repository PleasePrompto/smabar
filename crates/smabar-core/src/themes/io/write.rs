//! Serialized theme writes and their narrow rollback receipt.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::SmabarPaths;
use crate::util::lock_unpoisoned;

use super::super::{ThemeDocument, is_bundled, is_valid_theme_name};
use super::{THEME_WRITE_LOCK, ThemeIoError, document_to_json};

/// One completed file replacement that can be reverted if its paired config
/// write fails. Dropping it commits the replacement.
#[derive(Debug)]
pub struct PendingThemeWrite {
    target: PathBuf,
    previous: Option<Vec<u8>>,
    written: Vec<u8>,
    revision: u128,
}

impl PendingThemeWrite {
    /// Restores the exact previous bytes (or removes a newly created file).
    /// A newer, different write wins and is never overwritten.
    pub fn rollback(self) -> Result<bool, ThemeIoError> {
        let mut state = lock_unpoisoned(&THEME_WRITE_LOCK);
        if state.revisions.get(&self.target) != Some(&self.revision)
            || read_optional(&self.target)?.as_ref() != Some(&self.written)
        {
            return Ok(false);
        }
        match self.previous {
            Some(previous) => replace_file(&self.target, &previous)?,
            None => fs::remove_file(&self.target).map_err(|source| ThemeIoError::Io {
                action: "roll back",
                path: self.target.clone(),
                source,
            })?,
        }
        state.revisions.remove(&self.target);
        Ok(true)
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ThemeIoError> {
    match fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ThemeIoError::Io {
            action: "read",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn replace_file(target: &Path, content: &[u8]) -> Result<(), ThemeIoError> {
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, content).map_err(|source| ThemeIoError::Io {
        action: "write",
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, target).map_err(|source| ThemeIoError::Io {
        action: "replace",
        path: target.to_path_buf(),
        source,
    })
}

/// Writes a validated drop-in and returns its rollback receipt.
pub fn stage_theme_write(
    paths: &SmabarPaths,
    name: &str,
    document: &ThemeDocument,
    overwrite: bool,
) -> Result<PendingThemeWrite, ThemeIoError> {
    if !is_valid_theme_name(name) {
        return Err(ThemeIoError::InvalidName(name.to_string()));
    }
    if is_bundled(name) {
        return Err(ThemeIoError::BundledReadOnly(name.to_string()));
    }
    let written = document_to_json(document)?.into_bytes();
    let dir = paths.themes_dir();
    fs::create_dir_all(&dir).map_err(|source| ThemeIoError::Io {
        action: "create theme directory",
        path: dir.clone(),
        source,
    })?;
    let mut state = lock_unpoisoned(&THEME_WRITE_LOCK);
    let target = dir.join(format!("{name}.json"));
    let previous = read_optional(&target)?;
    if !overwrite && previous.is_some() {
        return Err(ThemeIoError::Exists(name.to_string()));
    }
    replace_file(&target, &written)?;
    state.next_revision += 1;
    let revision = state.next_revision;
    state.revisions.insert(target.clone(), revision);
    Ok(PendingThemeWrite {
        target,
        previous,
        written,
        revision,
    })
}

/// Writes a drop-in and commits it immediately.
pub fn write_theme(
    paths: &SmabarPaths,
    name: &str,
    document: &ThemeDocument,
    overwrite: bool,
) -> Result<PathBuf, ThemeIoError> {
    let pending = stage_theme_write(paths, name, document, overwrite)?;
    Ok(pending.target)
}

/// Deletes a drop-in. Bundled themes are read-only.
pub fn delete_theme(paths: &SmabarPaths, name: &str) -> Result<(), ThemeIoError> {
    if !is_valid_theme_name(name) {
        return Err(ThemeIoError::InvalidName(name.to_string()));
    }
    if is_bundled(name) {
        return Err(ThemeIoError::BundledReadOnly(name.to_string()));
    }
    let file = paths.themes_dir().join(format!("{name}.json"));
    let mut state = lock_unpoisoned(&THEME_WRITE_LOCK);
    match fs::remove_file(&file) {
        Ok(()) => {
            state.revisions.remove(&file);
            // A Community Theme's receipt belongs to the file.
            crate::store::receipts::forget_theme(paths, name);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(ThemeIoError::NotFound(
            format!("no drop-in theme \"{name}\""),
        )),
        Err(source) => Err(ThemeIoError::Io {
            action: "delete",
            path: file,
            source,
        }),
    }
}
