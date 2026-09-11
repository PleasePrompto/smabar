//! Response handling shared by every HTTP edge in the app (fonts, store).

use anyhow::{Context, bail};

/// Reads a body of at most `maximum` bytes; larger bodies are refused, not
/// truncated, so a caller never works on a partial file.
pub(crate) async fn read_limited(
    mut response: reqwest::Response,
    maximum: usize,
    kind: &str,
) -> anyhow::Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        bail!("{kind} exceeded the size limit");
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .map_or(0, |size| size.min(maximum as u64) as usize),
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("cannot read {kind}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            bail!("{kind} exceeded the size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        bail!("{kind} was empty or exceeded the size limit");
    }
    Ok(bytes)
}

pub(crate) fn content_type_is(headers: &reqwest::header::HeaderMap, expected: &str) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
}
