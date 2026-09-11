//! Consistent, self-contained theme exports.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::SmabarPaths;
use crate::util::lock_unpoisoned;

use super::super::{BUNDLED, ThemeDocument, ThemeSettings, bundled_default, is_valid_theme_name};
use super::{THEME_WRITE_LOCK, ThemeIoError, document_to_json, parse_document_strict};

fn export_document_unlocked(
    paths: &SmabarPaths,
    name: &str,
) -> Result<ThemeDocument, ThemeIoError> {
    if !is_valid_theme_name(name) {
        return Err(ThemeIoError::InvalidName(name.to_string()));
    }
    let bundled = BUNDLED.iter().find(|(known, _)| *known == name);
    let file = paths.themes_dir().join(format!("{name}.json"));
    let dropin = match fs::read_to_string(&file) {
        Ok(raw) => Some(parse_document_strict(&raw).map_err(ThemeIoError::InvalidDocument)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(ThemeIoError::Io {
                action: "read",
                path: file,
                source,
            });
        }
    };
    if bundled.is_none() && dropin.is_none() {
        return Err(ThemeIoError::NotFound(format!("no theme \"{name}\"")));
    }

    let mut tokens = bundled
        .map(|(_, theme)| theme.tokens.clone())
        .unwrap_or_else(|| bundled_default().clone());
    if let Some(dropin) = &dropin {
        tokens.extend(dropin.tokens.clone());
    }
    crate::fonts::canonicalize_theme_fonts(&mut tokens);

    let mut settings: ThemeSettings = BUNDLED
        .iter()
        .find(|(known, _)| *known == "default")
        .map(|(_, theme)| theme.settings.clone())
        .unwrap_or_default();
    if let Some((_, theme)) = bundled {
        settings.extend(theme.settings.clone());
    }
    if let Some(dropin) = &dropin {
        settings.extend(dropin.settings.clone());
    }
    let document = ThemeDocument {
        tokens,
        settings,
        meta: dropin.map(|dropin| dropin.meta).unwrap_or_default(),
    };
    document_to_json(&document)?;
    Ok(document)
}

/// Exports to `<name>.json`; collisions get `-2`, `-3`, … suffixes.
pub fn export_to_dir(
    paths: &SmabarPaths,
    name: &str,
    target_dir: &Path,
) -> Result<PathBuf, ThemeIoError> {
    let _write_guard = lock_unpoisoned(&THEME_WRITE_LOCK);
    let json = document_to_json(&export_document_unlocked(paths, name)?)?;
    fs::create_dir_all(target_dir).map_err(|source| ThemeIoError::Io {
        action: "create export directory",
        path: target_dir.to_path_buf(),
        source,
    })?;
    let mut target = target_dir.join(format!("{name}.json"));
    let mut counter = 2;
    while target.exists() {
        target = target_dir.join(format!("{name}-{counter}.json"));
        counter += 1;
    }
    fs::write(&target, json).map_err(|source| ThemeIoError::Io {
        action: "write",
        path: target.clone(),
        source,
    })?;
    Ok(target)
}
