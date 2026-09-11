//! Pinned shortcuts: native app discovery, original icon resolution, website
//! file, folder and special-item pins, and safe platform opening.
//!
//! The app edge injects the current platform. Linux executes only `Exec` data
//! from parsed `.desktop` files; ordinary files and folders go through the
//! native desktop opener. Curated special items and validated http(s) URLs use
//! fixed native openers. No untrusted source is interpreted as a command line.

pub(crate) mod desktop;
pub(crate) mod iconfile;
pub(crate) mod icons;
pub(crate) mod launch;
mod pins;
pub(crate) mod platform;
mod service;
pub mod web;

#[cfg(test)]
mod service_tests;

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;
use thiserror::Error;

pub use desktop::{DesktopApp, default_search_dirs};
pub use icons::{IconDirs, default_icon_dirs, icon_theme_from_settings_ini};
pub use pins::{pin_shortcut, unpin_shortcut};
pub use platform::{AppSource, InstalledApp, ShortcutPlatform};
pub use service::ShortcutsService;

/// A pinned shortcut resolved for display: label plus icon sources.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedShortcut {
    /// The generated pin id from the config.
    pub id: String,
    /// Label override, app/path/system-item name, or website host.
    pub label: String,
    /// Tile image sources in preference order: one `data:` URI (SVG/PNG)
    /// for local icons, the site's well-known icon URLs for website pins.
    /// The shell shows the first that loads and renders an initial-letter
    /// tile once the list is exhausted (empty = no icon at all).
    pub icons: Vec<String>,
    /// XDG desktop-file id when the pin references one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_id: Option<String>,
    /// True when this entry is a visual separator rather than a shortcut item.
    pub separator: bool,
}

/// Errors around pinning and launching shortcuts.
#[derive(Debug, Error)]
pub enum ShortcutError {
    /// A shortcut source needs either separator mode or exactly one launch source.
    #[error(
        "a shortcut needs separator=true with no source, or exactly one source: a desktopId, \
         an absolute supported path, an http(s) url, OR special=computer|trash"
    )]
    InvalidSource,
    /// Separators are visual only and cannot be launched.
    #[error("shortcut separator \"{id}\" cannot be launched")]
    SeparatorNotLaunchable { id: String },
    /// The desktop id was not found in the applications directories.
    #[error(
        "no application with desktop id \"{id}\" found in the applications directories; \
         use the exact source returned by app search (an absolute .desktop path is also \
         supported on Linux)"
    )]
    UnknownDesktopId { id: String },
    /// The pinned path is not an existing absolute file or directory.
    #[error(
        "shortcut path {path} must be an existing absolute file or folder; Linux .desktop \
         files must also be valid launchable application entries"
    )]
    InvalidDesktopPath { path: PathBuf },
    /// The pinned `.desktop` file could not be read.
    #[error("cannot read {path}: {source}")]
    ReadDesktopFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file parsed but is not a launchable application entry.
    #[error(
        "{path} is not a launchable desktop entry (needs a [Desktop Entry] section with \
         Type=Application, Name, and Exec)"
    )]
    NotAnApplication { path: PathBuf },
    /// Terminal applications are not supported in v1 (no terminal-emulator
    /// selection yet).
    #[error("\"{name}\" is a terminal application; launching terminal apps is not supported yet")]
    TerminalApp { name: String },
    /// The `Exec` line was empty after stripping field codes.
    #[error("\"{name}\" has an empty Exec command after removing field codes")]
    EmptyExec { name: String },
    /// The pinned website URL is not an acceptable http(s) link.
    #[error(transparent)]
    InvalidUrl(#[from] crate::open::OpenUrlError),
    /// Spawning the process failed.
    #[error("failed to launch \"{name}\": {source}")]
    Spawn {
        name: String,
        #[source]
        source: std::io::Error,
    },
    /// A native platform operation failed after validation.
    #[error("cannot {action}: {source}")]
    Platform {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },
    /// The same shortcut source is already pinned.
    #[error("this shortcut is already pinned (id \"{id}\")")]
    AlreadyPinned { id: String },
    /// No pinned shortcut carries this id.
    #[error("no pinned shortcut with id \"{id}\"; list the pins to see valid ids")]
    UnknownPin { id: String },
}
