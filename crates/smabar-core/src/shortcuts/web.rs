//! Website pins: display label and icon candidates derived from the pinned
//! URL.
//!
//! The icons are the site's OWN well-known icon paths. The core neither knows
//! nor cares who fetches them: the app edge downloads them into
//! `~/.smabar/cache/icons/` (that is where the HTTP client and the TLS stack
//! live), and the webview loads the remote URLs directly until a cached file
//! exists. So there is still no HTTP client in the core and no third-party
//! favicon service in the request path.
//! `/apple-touch-icon.png` comes first because `/favicon.ico` is routinely
//! 16x16 (bild.de) and looks mushy on a 32–48px tile that the fisheye then
//! magnifies; the touch icon is 60–180px. The shell walks the list on load
//! errors and ends at the initial-letter tile; a `label` override on the
//! pin covers the display name.

use std::fs;
use std::path::{Path, PathBuf};

use super::iconfile;

/// Host (plus port) of an http(s) URL that passed
/// [`crate::open::validate_url`], lowercased and without userinfo.
fn host(url: &str) -> &str {
    let rest = url
        .split_once("://")
        .map_or(url, |(_scheme, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // `user:pass@host` — everything up to the last `@` is userinfo.
    rest.rsplit_once('@').map_or(rest, |(_userinfo, host)| host)
}

/// Default label of a website pin: the host without a leading `www.`.
pub(super) fn label(url: &str) -> String {
    let host = host(url).to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// Tile image sources in preference order, highest resolution first. The
/// shell falls through to the next one whenever a candidate fails to load
/// (404, soft-404 HTML, offline).
pub fn icon_candidates(url: &str) -> Vec<String> {
    let host = host(url).to_ascii_lowercase();
    vec![
        format!("https://{host}/apple-touch-icon.png"),
        format!("https://{host}/favicon.ico"),
    ]
}

/// Cache file stem for a pinned URL: the lowercased host reduced to
/// `[a-z0-9.-]`; everything else (port separators, unicode, and in
/// particular the `\` of a Windows path-traversal trick like
/// `https://evil.com\..\x`) becomes `_`. No path separator survives, so
/// `icons_dir.join(format!("{stem}.{extension}"))` can never leave the
/// cache directory. Two exotic hosts mapping to the same stem share a
/// cached icon — a cosmetic collision, not a security issue.
pub fn icon_stem(url: &str) -> String {
    host(url)
        .to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '.' | '-' => c,
            _ => '_',
        })
        .collect()
}

/// The downloaded icon of a website pin, inlined as a data URI.
///
/// Returns `None` while nothing has been downloaded yet — the caller then
/// falls back to [`icon_candidates`], which is also what fills this cache.
pub(super) fn cached_icon(icons_dir: &Path, url: &str) -> Option<String> {
    cached_icon_path(icons_dir, url).and_then(|path| iconfile::data_uri_for_file(&path))
}

/// Shared cache lookup for the downloader and resolver. Incomplete `.part`
/// files and unsupported image types must not suppress another download.
pub fn cached_icon_path(icons_dir: &Path, url: &str) -> Option<PathBuf> {
    let stem = icon_stem(url);
    let entries = fs::read_dir(icons_dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.file_stem().and_then(|name| name.to_str()) == Some(stem.as_str())
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| iconfile::mime_for_extension(ext).is_some())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_the_host_without_www() {
        assert_eq!(label("https://www.bild.de/"), "bild.de");
        assert_eq!(
            label("https://news.ycombinator.com"),
            "news.ycombinator.com"
        );
        assert_eq!(label("HTTP://Example.COM/path?q=1#f"), "example.com");
        assert_eq!(label("https://user@example.com/x"), "example.com");
        assert_eq!(label("http://localhost:8080/dash"), "localhost:8080");
    }

    #[test]
    fn the_cache_stem_is_a_valid_file_name_even_with_a_port() {
        assert_eq!(icon_stem("https://EXAMPLE.com/x"), "example.com");
        assert_eq!(icon_stem("http://localhost:8080/dash"), "localhost_8080");
    }

    #[test]
    fn the_cache_stem_neutralizes_path_traversal_tricks() {
        // Windows traversal through a backslash host: without the charset
        // reduction this became `evil.com\..\..\Users\Public` and escaped
        // the cache directory on join.
        assert_eq!(
            icon_stem(r"https://evil.com\..\..\Users\Public\Foo"),
            "evil.com_.._.._users_public_foo"
        );
        // Every stem stays inside its directory: no separator survives.
        for url in [
            r"https://a\b/../c",
            "https://../../x",
            r"https://%2e%2e\win.ini",
        ] {
            let stem = icon_stem(url);
            assert!(!stem.contains(['/', '\\']), "stem: {stem}");
        }
    }

    #[test]
    fn a_downloaded_icon_is_served_from_the_cache_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(cached_icon(dir.path(), "https://example.com/").is_none());
        fs::write(dir.path().join("example.com.part"), b"partial").expect("partial download");
        assert!(cached_icon_path(dir.path(), "https://example.com/").is_none());
        fs::write(dir.path().join("example.com.png"), b"\x89PNG").expect("write cached icon");
        assert_eq!(
            cached_icon(dir.path(), "https://example.com/").as_deref(),
            Some("data:image/png;base64,iVBORw==")
        );
        // A stray non-image next to it must not be inlined.
        assert!(cached_icon(dir.path(), "https://other.example/").is_none());
    }

    #[test]
    fn icon_candidates_prefer_the_touch_icon_over_the_favicon() {
        assert_eq!(
            icon_candidates("https://www.bild.de/politik/artikel"),
            vec![
                "https://www.bild.de/apple-touch-icon.png",
                "https://www.bild.de/favicon.ico",
            ]
        );
        // Always https, even for an http pin — icons are cosmetic and must
        // not downgrade the page the browser later opens.
        assert_eq!(
            icon_candidates("http://example.com")[0],
            "https://example.com/apple-touch-icon.png"
        );
    }
}
