//! Icon file → data URI.
//!
//! Split out of `service.rs` for the 500-line limit, and because two callers
//! need it now: application icons resolved from the icon theme, and website
//! icons downloaded into the cache.

use std::fs;
use std::path::Path;

use base64::Engine as _;

/// Icon files above this size are not inlined as data URIs.
pub(crate) const MAX_ICON_BYTES: u64 = 512 * 1024;

/// The image types accepted for inlining, by file extension.
///
/// SVG and PNG cover the icon theme; the rest is what a website actually
/// serves for `/favicon.ico` and `/apple-touch-icon.png`.
pub(super) fn mime_for_extension(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "ico" => Some("image/x-icon"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Inlines an icon file as a data URI, capped at [`MAX_ICON_BYTES`].
pub(crate) fn data_uri_for_file(path: &Path) -> Option<String> {
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(mime_for_extension)
    {
        Some(mime) => mime,
        None => {
            tracing::debug!(path = %path.display(), "unsupported icon extension");
            return None;
        }
    };
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_ICON_BYTES => {
            tracing::debug!(path = %path.display(), size = metadata.len(), "icon exceeds inline cap");
            return None;
        }
        Ok(_) => {}
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "cannot stat icon file");
            return None;
        }
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "cannot read icon file");
            return None;
        }
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_icon_types_are_inlinable_and_unknown_ones_are_not() {
        assert_eq!(mime_for_extension("ICO"), Some("image/x-icon"));
        assert_eq!(mime_for_extension("jpeg"), Some("image/jpeg"));
        assert_eq!(mime_for_extension("svg"), Some("image/svg+xml"));
        assert_eq!(mime_for_extension("html"), None);
        assert_eq!(mime_for_extension(""), None);
    }

    #[test]
    fn a_file_over_the_cap_is_refused_instead_of_inlined() {
        let dir = tempfile::tempdir().expect("temp dir");
        let big = dir.path().join("huge.png");
        fs::write(&big, vec![0u8; (MAX_ICON_BYTES + 1) as usize]).expect("write icon");
        assert!(data_uri_for_file(&big).is_none());
    }

    #[test]
    fn a_small_png_becomes_a_data_uri() {
        let dir = tempfile::tempdir().expect("temp dir");
        let icon = dir.path().join("small.png");
        fs::write(&icon, b"\x89PNG").expect("write icon");
        assert_eq!(
            data_uri_for_file(&icon).as_deref(),
            Some("data:image/png;base64,iVBORw==")
        );
    }
}
