//! Errors shared by Community Store operations and their callers.

use std::path::PathBuf;

use super::{CatalogError, FetchError, archive};

/// Why a store operation failed. Every message names the next step.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    LocalArchive(String),
    #[error("{0}")]
    Fetch(#[from] FetchError),
    #[error("{0}")]
    Catalog(#[from] CatalogError),
    #[error("no catalog is available yet; refresh the store while online")]
    NoCatalog,
    #[error("\"{id}\" is not listed in the catalog; refresh the store")]
    Unknown { id: String },
    #[error("{what} of \"{id}\" does not match its listed SHA-256; refresh and try again")]
    DigestMismatch { what: &'static str, id: String },
    #[error("SMABAR_STORE_ENDPOINT is not a usable base URL: {0}")]
    Endpoint(String),
    #[error("the store could not be reached ({0}); installing needs a fresh catalog — try again")]
    StoreUnreachable(String),
    #[error("another install is running; wait for it to finish")]
    Busy,
    #[error("\"{id}\" is not a valid id; ids contain only [a-z0-9-]")]
    InvalidId { id: String },
    #[error(
        "the listing of \"{id}\" changed since you looked (expected {expected}, now {actual}); refresh and try again"
    )]
    VersionChanged {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("\"{id}\" ships with smabar; bundled plugins cannot be replaced")]
    BasePlugin { id: String },
    #[error(
        "\"{id}\" exists as your own plugin in {} without a store receipt; the store never replaces it — remove it first",
        path.display()
    )]
    UserPlugin { id: String, path: PathBuf },
    #[error("\"{id}\" {version} is blocked by the store: {reason}")]
    Blocked {
        id: String,
        version: String,
        reason: String,
    },
    #[error("\"{id}\" needs {requirement}; this smabar is {app_version} on {host_os}")]
    Incompatible {
        id: String,
        requirement: String,
        app_version: String,
        host_os: String,
    },
    #[error(
        "\"{id}\" was edited locally since the store installed it; confirm to replace it (a backup is kept in {})",
        backup.display()
    )]
    LocallyModified { id: String, backup: PathBuf },
    #[error(
        "the listing of \"{id}\" violates the catalog contract ({detail}); refresh, and report this if it persists"
    )]
    Contract { id: String, detail: String },
    #[error(
        "the downloaded archive of \"{id}\" does not match the listed tree ({expected} vs {actual}); \
         the repository may use export-ignore or eol attributes — nothing was installed"
    )]
    TreeMismatch {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("{0}")]
    Archive(#[from] archive::ArchiveError),
    #[error("the downloaded smabar.json is invalid: {0}")]
    Manifest(#[from] crate::plugins::ManifestError),
    #[error("the downloaded manifest declares id \"{actual}\", the listing says \"{expected}\"")]
    IdMismatch { expected: String, actual: String },
    #[error(
        "the downloaded manifest of \"{id}\" says version {manifest}, the listing says {listed}"
    )]
    VersionMismatch {
        id: String,
        listed: String,
        manifest: String,
    },
    #[error(
        "\"{id}\" names the entry script \"{entry}\", which is not a file inside the plugin folder"
    )]
    EntryMissing { id: String, entry: String },
    #[error("{0}")]
    Replace(#[from] crate::plugins::ReplaceError),
    #[error("\"{id}\" failed to start after the install ({reason}); {}", if *rolled_back { "the previous state was restored" } else { "check the backup folder" })]
    StartFailed {
        id: String,
        reason: String,
        rolled_back: bool,
    },
    #[error("{0}")]
    Theme(#[from] crate::themes::io::ThemeIoError),
    #[error("\"{name}\" is a theme compiled into smabar; it cannot be installed from the store")]
    BundledTheme { name: String },
    #[error(
        "a theme named \"{name}\" exists in {} without a store receipt; the store never replaces it — delete it first",
        path.display()
    )]
    LocalTheme { name: String, path: PathBuf },
    #[error(
        "\"{id}\" is already installed; the catalog has no newer or changed version to install"
    )]
    NoUpdate { id: String },
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
