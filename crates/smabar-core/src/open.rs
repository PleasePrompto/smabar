//! Validation for http(s) links opened through the injected native shortcut
//! platform. The fixed platform opener receives exactly this one validated
//! argument; plugin HTML can never provide a command line.

use thiserror::Error;

/// Upper bound for accepted URLs; anything longer is suspicious, not a link.
const MAX_URL_LEN: usize = 2048;

/// Errors from [`open_url`].
#[derive(Debug, Error)]
pub enum OpenUrlError {
    /// The URL is not an acceptable http(s) link.
    #[error("refusing to open \"{url}\": {reason}")]
    InvalidUrl { url: String, reason: &'static str },
}

/// Validates that `url` is a plain http(s) link: correct scheme, non-empty
/// host, no whitespace/control characters, bounded length.
pub fn validate_url(url: &str) -> Result<(), OpenUrlError> {
    let invalid = |reason: &'static str| OpenUrlError::InvalidUrl {
        // Truncate for the error message; logs must not carry huge blobs.
        url: url.chars().take(120).collect(),
        reason,
    };
    if url.len() > MAX_URL_LEN {
        return Err(invalid("URL is longer than 2048 characters"));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid("URL contains whitespace or control characters"));
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(|| invalid("only http:// and https:// URLs open externally"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err(invalid("URL has no host"));
    }
    // A backslash host is a Windows path-traversal trick, not a URL host.
    if host.contains('\\') {
        return Err(invalid("URL host contains a backslash"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_http_and_https_links() {
        for url in [
            "https://example.com",
            "http://example.com/path?q=1#frag",
            "HTTPS://EXAMPLE.COM/Path",
        ] {
            assert!(validate_url(url).is_ok(), "url: {url}");
        }
    }

    #[test]
    fn rejects_other_schemes_hostless_and_noisy_urls() {
        for url in [
            "",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>x</script>",
            "ftp://example.com",
            "https://",
            "https:///path-only",
            "https://exa mple.com",
            "https://example.com/\u{0009}tab",
            r"https://evil.com\..\..\Users\Public",
            "example.com",
        ] {
            assert!(validate_url(url).is_err(), "url: {url}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(validate_url(&long).is_err());
    }

    #[test]
    fn error_messages_truncate_the_url() {
        let url = format!("ftp://{}", "x".repeat(3000));
        let error = validate_url(&url).expect_err("scheme refused");
        assert!(error.to_string().len() < 300);
    }
}
