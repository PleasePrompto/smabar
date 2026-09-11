//! Shared theme-file I/O: strict parsing, atomic writes, delete, export,
//! import, and materializing the current look as a drop-in.
//!
//! The MCP `theme_write` tool and the Tauri theme commands both go through
//! this module, so validation and the on-disk format have exactly one home.
//! The tolerant parser in [`super`] stays separate on purpose: resolving a
//! broken drop-in must never break the bar, while import/write must reject
//! it with every problem named.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use thiserror::Error;

use crate::config::{SmabarConfig, SmabarPaths};

use super::settings::{ALLOWED_PATHS, parse_settings_value, validate_settings};
use super::{
    META_KEY, SETTINGS_KEY, ThemeDocument, ThemeMap, ThemeMeta, ThemeSettings, contract,
    is_bundled, resolve,
};

mod export;
mod write;
pub use export::export_to_dir;
pub use write::{PendingThemeWrite, delete_theme, stage_theme_write, write_theme};

/// Import refuses files above this size — a theme document is a few KiB.
pub const IMPORT_MAX_BYTES: u64 = 512 * 1024;

/// Meta fields are short human-readable strings, not documents.
const META_MAX_CHARS: usize = 200;

#[derive(Default)]
struct ThemeWriteState {
    next_revision: u128,
    revisions: BTreeMap<PathBuf, u128>,
}

/// ponytail: one process-wide lock and generation map keep file transactions
/// simple; split them by canonical path only if measured contention appears.
static THEME_WRITE_LOCK: LazyLock<Mutex<ThemeWriteState>> = LazyLock::new(Default::default);

/// Errors of the theme-file operations, with actionable Display texts.
#[derive(Debug, Error)]
pub enum ThemeIoError {
    /// The name violates the `[a-z0-9-]` file-stem rule.
    #[error("theme name \"{0}\" must be non-empty and contain only [a-z0-9-]")]
    InvalidName(String),
    /// The name belongs to a compiled-in theme (write/delete refused).
    #[error("\"{0}\" is a compiled-in theme and read-only")]
    BundledReadOnly(String),
    /// A drop-in with this name exists and `overwrite` was false.
    #[error("a theme named \"{0}\" already exists")]
    Exists(String),
    /// No such theme (delete/export).
    #[error("{0}")]
    NotFound(String),
    /// The document failed strict validation; every problem is listed.
    #[error("invalid theme document: {}", .0.join("; "))]
    InvalidDocument(Vec<String>),
    /// A theme document is not syntactically valid JSON.
    #[error("invalid theme document {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// Filesystem access failed.
    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Serializing a validated in-memory document failed.
    #[error("cannot serialize {what}: {source}")]
    Serialize {
        what: &'static str,
        #[source]
        source: serde_json::Error,
    },
}

/// Accepts only a real JSON object of string values and validates every
/// token; a JSON object arriving as an escaped string is rejected with a
/// pointed message (some MCP clients serialize nested objects that way).
pub fn parse_tokens_strict(tokens: Value) -> Result<ThemeMap, String> {
    let object = match tokens {
        Value::Object(object) => object,
        Value::String(raw) if serde_json::from_str::<Value>(&raw).is_ok_and(|v| v.is_object()) => {
            return Err(
                "`tokens` arrived as a JSON-encoded string; pass a real JSON object".to_string(),
            );
        }
        _ => {
            return Err(
                "`tokens` must be a JSON object mapping CSS custom-property names to string \
                 values"
                    .to_string(),
            );
        }
    };
    let mut map = ThemeMap::new();
    for (key, value) in object {
        let Value::String(value) = value else {
            return Err(format!("token \"{key}\" must have a string value"));
        };
        if let Err(reason) = contract::validate_token(&key, &value) {
            return Err(format!("invalid token \"{key}\": {reason}"));
        }
        map.insert(key, value);
    }
    Ok(map)
}

/// Strictly validates a `meta` block: an object whose known fields (name,
/// author, version, description) are short plain strings. Unknown fields are
/// ignored — a newer store may add some.
pub fn parse_meta_value(value: Value) -> Result<ThemeMeta, Vec<String>> {
    let Value::Object(block) = value else {
        return Err(vec![
            "\"meta\" must be a JSON object of string fields".to_string(),
        ]);
    };
    let mut meta = ThemeMeta::default();
    let mut errors = Vec::new();
    for (field, slot) in [
        ("name", &mut meta.name),
        ("author", &mut meta.author),
        ("version", &mut meta.version),
        ("description", &mut meta.description),
    ] {
        let Some(value) = block.get(field) else {
            continue;
        };
        match value {
            Value::String(text)
                if !text.is_empty()
                    && text.chars().count() <= META_MAX_CHARS
                    && !text.chars().any(char::is_control) =>
            {
                *slot = Some(text.clone());
            }
            _ => errors.push(format!(
                "meta.{field} must be a plain string of 1-{META_MAX_CHARS} characters"
            )),
        }
    }
    if errors.is_empty() {
        Ok(meta)
    } else {
        Err(errors)
    }
}

/// Parses one whole theme file strictly, collecting EVERY problem instead of
/// stopping at the first. Unknown non-token top-level keys (e.g. `$schema`,
/// future reserved keys) are ignored for forward compatibility.
pub fn parse_document_strict(raw: &str) -> Result<ThemeDocument, Vec<String>> {
    let entries: serde_json::Map<String, Value> =
        serde_json::from_str(raw).map_err(|error| vec![format!("not a JSON object: {error}")])?;
    let mut document = ThemeDocument::default();
    let mut errors = Vec::new();
    for (key, value) in entries {
        if key == SETTINGS_KEY {
            match parse_settings_value(value) {
                Ok(block) => {
                    if let Err(problems) = validate_settings(&block) {
                        errors.extend(problems);
                    }
                    document.settings = block;
                }
                Err(message) => errors.push(message),
            }
            continue;
        }
        if key == META_KEY {
            match parse_meta_value(value) {
                Ok(meta) => document.meta = meta,
                Err(problems) => errors.extend(problems),
            }
            continue;
        }
        if !key.starts_with("--") {
            continue;
        }
        let Value::String(value) = value else {
            errors.push(format!("token \"{key}\" must have a string value"));
            continue;
        };
        match contract::validate_token(&key, &value) {
            Ok(()) => {
                document.tokens.insert(key, value);
            }
            Err(reason) => errors.push(format!("invalid token \"{key}\": {reason}")),
        }
    }
    if errors.is_empty() {
        Ok(document)
    } else {
        Err(errors)
    }
}

/// Serializes a document in the canonical on-disk shape: sorted flat tokens
/// plus the reserved blocks (only when non-empty), pretty-printed with a
/// trailing newline.
fn document_to_json(document: &ThemeDocument) -> Result<String, ThemeIoError> {
    let mut file: serde_json::Map<String, Value> = document
        .tokens
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    if !document.settings.is_empty() {
        file.insert(
            SETTINGS_KEY.to_string(),
            Value::Object(document.settings.clone().into_iter().collect()),
        );
    }
    if !document.meta.is_empty() {
        let meta =
            serde_json::to_value(&document.meta).map_err(|source| ThemeIoError::Serialize {
                what: "theme metadata",
                source,
            })?;
        file.insert(META_KEY.to_string(), meta);
    }
    let json = serde_json::to_string_pretty(&file)
        .map(|json| format!("{json}\n"))
        .map_err(|source| ThemeIoError::Serialize {
            what: "theme",
            source,
        })?;
    parse_document_strict(&json).map_err(ThemeIoError::InvalidDocument)?;
    Ok(json)
}

/// The current behavior of `config` as a theme settings block: every allowed
/// path with its live value. By construction complete (all allowed paths),
/// so a saved theme re-activates as a no-op right after saving.
pub fn snapshot_settings(config: &SmabarConfig) -> ThemeSettings {
    let serialized = serde_json::to_value(config).unwrap_or_default();
    ALLOWED_PATHS
        .iter()
        .filter_map(|path| {
            let pointer = format!("/{}", path.replace('.', "/"));
            serialized
                .pointer(&pointer)
                .map(|value| ((*path).to_string(), value.clone()))
        })
        .collect()
}

/// The user's current look as a complete theme document: the active theme's
/// resolved tokens overlaid with the `appearance.tokens` slider overrides,
/// plus a full snapshot of the live behavior settings.
pub fn current_look_document(paths: &SmabarPaths, config: &SmabarConfig) -> ThemeDocument {
    let mut tokens = resolve(paths, &config.theme);
    tokens.extend(config.appearance.tokens.clone());
    crate::fonts::canonicalize_theme_fonts(&mut tokens);
    ThemeDocument {
        tokens,
        settings: snapshot_settings(config),
        meta: ThemeMeta::default(),
    }
}

/// Stages the same current-look write but keeps enough information to undo it
/// if the following config persistence fails.
pub fn stage_current_theme(
    paths: &SmabarPaths,
    config: &SmabarConfig,
    name: &str,
    overwrite: bool,
) -> Result<PendingThemeWrite, ThemeIoError> {
    stage_theme_write(
        paths,
        name,
        &current_look_document(paths, config),
        overwrite,
    )
}

/// Imports one theme file into the drop-in directory. The theme name is the
/// slugified file stem; bundled names are refused hard (importing a
/// `default.json` would otherwise silently PATCH the bundled default).
/// Returns the final theme name.
pub fn import_theme_file(
    paths: &SmabarPaths,
    file: &Path,
    overwrite: bool,
) -> Result<String, ThemeIoError> {
    let metadata = fs::metadata(file).map_err(|source| ThemeIoError::Io {
        action: "read",
        path: file.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ThemeIoError::NotFound(format!(
            "{} is not a regular file",
            file.display()
        )));
    }
    if metadata.len() > IMPORT_MAX_BYTES {
        return Err(ThemeIoError::InvalidDocument(vec![format!(
            "file is larger than {} KiB — not a theme document",
            IMPORT_MAX_BYTES / 1024
        )]));
    }
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let name =
        slugify_theme_name(stem).ok_or_else(|| ThemeIoError::InvalidName(stem.to_string()))?;
    if is_bundled(&name) {
        return Err(ThemeIoError::BundledReadOnly(name));
    }
    let raw = fs::read_to_string(file).map_err(|source| ThemeIoError::Io {
        action: "read",
        path: file.to_path_buf(),
        source,
    })?;
    serde_json::from_str::<serde_json::Map<String, Value>>(&raw).map_err(|source| {
        ThemeIoError::Parse {
            path: file.to_path_buf(),
            source,
        }
    })?;
    let document = parse_document_strict(&raw).map_err(ThemeIoError::InvalidDocument)?;
    if document.tokens.is_empty() && document.settings.is_empty() {
        return Err(ThemeIoError::InvalidDocument(vec![
            "the file contains no theme tokens and no settings block — not a theme document"
                .to_string(),
        ]));
    }
    write_theme(paths, &name, &document, overwrite)?;
    Ok(name)
}

/// Reduces arbitrary input (a typed name, a file stem) to a valid theme
/// name: lowercase, runs of anything outside `[a-z0-9]` collapse to one
/// dash, capped at 64 characters. `None` when nothing usable remains.
pub fn slugify_theme_name(input: &str) -> Option<String> {
    let mut slug = String::new();
    let mut pending_dash = false;
    for c in input.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(c);
        } else {
            pending_dash = true;
        }
    }
    slug.truncate(64);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}
