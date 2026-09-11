//! The network seam of the store: what the app edge implements with an HTTP
//! client, and what the core asks of it. `smabar-core` carries no network
//! stack, so every byte enters through [`StoreFetcher`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use thiserror::Error;
use url::Url;

/// Upper bound for `catalog-v1.json`; 2 KiB per listed item leaves room
/// for thousands.
pub const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
/// A minisign signature file is four short lines.
pub const MAX_SIGNATURE_BYTES: usize = 4 * 1024;
/// A detail file carries a README of at most 20 000 characters.
pub const MAX_DETAIL_BYTES: usize = 1024 * 1024;
/// Theme files share the limit of the theme import path.
pub const MAX_THEME_BYTES: usize = crate::themes::io::IMPORT_MAX_BYTES as usize;
/// A commit archive is the whole repository, compressed. Plugins are text;
/// this leaves room for icons and stops a repository used as a CDN.
pub const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024;

/// A successful GET.
#[derive(Debug, Clone)]
pub struct FetchBody {
    pub bytes: Vec<u8>,
    /// The `ETag` header verbatim, sent back as `If-None-Match` next time.
    pub etag: Option<String>,
}

/// What a conditional GET came back with.
#[derive(Debug, Clone)]
pub enum FetchOutcome {
    /// `304 Not Modified`: the caller's copy is current.
    NotModified,
    Body(FetchBody),
}

/// Why a fetch failed; each message names what to check.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("no connection to {host}: {reason}; check the network or SMABAR_STORE_ENDPOINT")]
    Offline { host: String, reason: String },
    #[error("{url} answered HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{what} is larger than the {limit}-byte limit; refusing to read it")]
    TooLarge { what: &'static str, limit: u64 },
    #[error("cannot write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

pub type FetchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, FetchError>> + Send + 'a>>;
/// `(received bytes, total bytes if known)`.
pub type ProgressFn<'a> = &'a (dyn Fn(u64, Option<u64>) + Send + Sync);

/// HTTP as the store needs it. Implemented with reqwest in the app crate and
/// with an in-memory fake in tests.
pub trait StoreFetcher: Send + Sync + 'static {
    /// GET `url`, sending `if_none_match` verbatim; bodies above `max_bytes`
    /// are refused, not truncated.
    fn get<'a>(
        &'a self,
        url: &'a Url,
        if_none_match: Option<&'a str>,
        max_bytes: usize,
    ) -> FetchFuture<'a, FetchOutcome>;

    /// Streams `url` into `target` (a temp file plus rename), stopping at
    /// `max_bytes`, reporting progress as `(received, total)`. Resolves to the
    /// byte count written.
    fn download<'a>(
        &'a self,
        url: &'a Url,
        target: &'a Path,
        max_bytes: u64,
        progress: ProgressFn<'a>,
    ) -> FetchFuture<'a, u64>;
}

/// The archive URL the catalog contract prescribes for a commit; an entry
/// whose `archiveUrl` differs is refused before any request is made.
pub fn expected_archive_url(name_with_owner: &str, commit: &str) -> String {
    format!("https://github.com/{name_with_owner}/archive/{commit}.zip")
}

/// The raw-file URL the catalog contract prescribes for a theme file.
pub fn expected_file_url(name_with_owner: &str, commit: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{name_with_owner}/{commit}/{path}")
}

/// Whether `commit` is a full lowercase SHA-1, the only form archives use.
pub fn is_full_commit(commit: &str) -> bool {
    commit.len() == 40
        && commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
