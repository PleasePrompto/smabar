//! Downloads the icons of website pins into `~/.smabar/cache/icons/`.
//!
//! This lives at the app edge on purpose: `smabar-core` carries no HTTP
//! client and no TLS stack (see `shortcuts/web.rs`). Until a file exists the
//! bar keeps showing the remote candidates, so the download is pure
//! improvement — once it lands the icon renders instantly and offline.
//!
//! Why it is needed at all: the first candidate is `/apple-touch-icon.png`,
//! and a site that does not have one answers with its regular 404 PAGE. The
//! webview downloads that whole document before `onerror` fires and it can
//! move to the next candidate — a visibly broken tile on every cold start.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail, ensure};
use smabar_core::config::SmabarConfig;
use smabar_core::shortcuts::{ShortcutsService, web};
use tauri::{AppHandle, Emitter};

/// A favicon is small; anything larger is a soft-404 page or a trap.
const MAX_ICON_BYTES: usize = 512 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 10;

/// Downloads whatever is still missing, then tells the shell to refetch.
///
/// Never fails loudly: a pin without a reachable icon simply keeps using the
/// remote candidates and the initial-letter fallback.
pub fn spawn_fetch(app: AppHandle, service: ShortcutsService, config: &SmabarConfig) {
    let urls: Vec<String> = config
        .shortcuts
        .pinned
        .iter()
        .filter(|entry| !entry.separator)
        .filter_map(|entry| entry.url.clone())
        .collect();
    if urls.is_empty() {
        return;
    }
    let icons_dir = service.icons_dir().to_path_buf();
    tauri::async_runtime::spawn(async move {
        if fetch_missing(&icons_dir, &urls).await {
            // The config did not change, so this cannot loop back into the
            // config watcher — the shell just refetches the resolved pins.
            if let Err(error) = app.emit("shortcuts-changed", ()) {
                tracing::warn!(%error, "cannot notify the shell about downloaded website icons");
            }
        }
    });
}

/// Fetches every URL that has no cached icon yet. Returns true when at least
/// one new file landed.
async fn fetch_missing(icons_dir: &Path, urls: &[String]) -> bool {
    let pending: Vec<&String> = urls
        .iter()
        .filter(|url| web::cached_icon_path(icons_dir, url).is_none())
        .collect();
    if pending.is_empty() {
        return false;
    }
    let client = match reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        // Icons are cosmetic and must never downgrade a connection.
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "cannot build the icon http client");
            return false;
        }
    };
    if let Err(error) = std::fs::create_dir_all(icons_dir) {
        tracing::warn!(path = %icons_dir.display(), %error, "cannot create the icon cache");
        return false;
    }

    let mut fetched = false;
    for url in pending {
        // The candidates are ordered best-first; the first real image wins.
        for candidate in resolved_candidates(&client, url).await {
            let Some((extension, bytes)) = download(&client, &candidate).await else {
                continue;
            };
            let target = icons_dir.join(format!("{}.{extension}", web::icon_stem(url)));
            match write_atomically(&target, &bytes) {
                Ok(()) => {
                    tracing::info!(
                        url = %candidate,
                        path = %target.display(),
                        bytes = bytes.len(),
                        "cached a website icon"
                    );
                    fetched = true;
                }
                Err(error) => {
                    tracing::warn!(path = %target.display(), %error, "cannot write a website icon")
                }
            }
            break;
        }
    }
    fetched
}

async fn resolved_candidates(client: &reqwest::Client, url: &str) -> Vec<String> {
    let original = web::icon_candidates(url);
    // A moved domain may redirect every path to its new homepage. Resolve
    // that origin before trying icon paths, without sending the pin's query
    // or credentials. The pinned browser URL itself remains unchanged.
    let root = reqwest::Url::parse(&original[0]).and_then(|url| url.join("/"));
    let result = match root {
        Ok(root) => get_https(client, root.as_str()).await,
        Err(error) => Err(error.into()),
    };
    match result {
        Ok(response) => {
            let mut candidates = web::icon_candidates(response.url().as_str());
            for candidate in original {
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
            candidates
        }
        Err(error) => {
            tracing::debug!(%error, "cannot resolve the website icon origin; trying original paths");
            original
        }
    }
}

/// Follow redirects over HTTPS only, including old domains whose Location
/// still says HTTP. Each hop is upgraded before any request is sent.
async fn get_https(client: &reqwest::Client, url: &str) -> anyhow::Result<reqwest::Response> {
    tokio::time::timeout(REQUEST_TIMEOUT, async {
        let mut current = reqwest::Url::parse(url)?;
        for redirects in 0..=MAX_REDIRECTS {
            let response = client.get(current.clone()).send().await?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                return Ok(response);
            };
            ensure!(
                redirects < MAX_REDIRECTS,
                "website icon redirect limit exceeded"
            );
            current = https_redirect(&current, location.to_str()?)?;
        }
        bail!("website icon redirect limit exceeded")
    })
    .await
    .context("website icon redirects timed out")?
}

fn https_redirect(current: &reqwest::Url, location: &str) -> anyhow::Result<reqwest::Url> {
    let mut next = current.join(location)?;
    ensure!(
        matches!(next.scheme(), "http" | "https")
            && next.username().is_empty()
            && next.password().is_none(),
        "website icon redirect must be an HTTP(S) URL without credentials"
    );
    next.set_scheme("https")
        .map_err(|()| anyhow::anyhow!("cannot secure the website icon redirect"))?;
    Ok(next)
}

/// One candidate URL → (file extension, bytes), or `None` when it is not a
/// usable image.
async fn download(client: &reqwest::Client, url: &str) -> Option<(&'static str, Vec<u8>)> {
    let response = match get_https(client, url).await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(%url, %error, "website icon request failed");
            return None;
        }
    };
    if !response.status().is_success() {
        tracing::debug!(%url, status = %response.status(), "website icon not available");
        return None;
    }
    // A soft 404 answers 200 with HTML — the content type is what separates
    // a real icon from a whole web page.
    let extension = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(extension_for_content_type)?;
    match crate::http_util::read_limited(response, MAX_ICON_BYTES, "website icon").await {
        Ok(bytes) => Some((extension, bytes)),
        Err(error) => {
            tracing::debug!(%url, %error, "website icon body rejected; trying the next candidate");
            None
        }
    }
}

/// The image types worth caching, keyed by response content type.
fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some("png"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        "image/jpeg" => Some("jpg"),
        "image/svg+xml" => Some("svg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

/// Write + rename, so the bar never reads a half-downloaded icon.
pub(crate) fn write_atomically(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = target.with_extension("part");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn download_response(
        headers: &str,
        body: Vec<u8>,
        keep_open: bool,
    ) -> Option<(&'static str, Vec<u8>)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let address = listener.local_addr().expect("test address");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n{headers}\r\n"
        );
        let (finish, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("test client");
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).await.expect("read request") > 0);
            stream
                .write_all(response.as_bytes())
                .await
                .expect("headers");
            stream.write_all(&body).await.expect("body");
            if keep_open {
                wait.await.expect("release stalled server");
            }
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            download(&client, &format!("http://{address}/icon.png")),
        )
        .await;
        let _ = finish.send(());
        server.await.expect("test server");
        result.expect("download must reject oversized bodies before the server finishes")
    }

    #[tokio::test]
    async fn icon_bodies_are_limited_while_reading() {
        let allowed = vec![b'x'; MAX_ICON_BYTES];
        assert_eq!(
            download_response(
                &format!("Content-Length: {MAX_ICON_BYTES}\r\n"),
                allowed.clone(),
                false,
            )
            .await,
            Some(("png", allowed)),
        );
        assert!(
            download_response(
                &format!("Content-Length: {}\r\n", MAX_ICON_BYTES + 1),
                Vec::new(),
                true,
            )
            .await
            .is_none()
        );
        let oversized = vec![b'x'; MAX_ICON_BYTES + 1];
        assert!(
            download_response("", oversized.clone(), true)
                .await
                .is_none()
        );
        let mut chunked = format!("{:x}\r\n", oversized.len()).into_bytes();
        chunked.extend_from_slice(&oversized);
        chunked.extend_from_slice(b"\r\n");
        assert!(
            download_response("Transfer-Encoding: chunked\r\n", chunked, true)
                .await
                .is_none()
        );
        assert!(
            download_response("Content-Length: 0\r\n", Vec::new(), false)
                .await
                .is_none()
        );
        assert!(
            download_response("Content-Length: 10\r\n", b"short".to_vec(), false)
                .await
                .is_none()
        );
    }

    #[test]
    fn only_real_image_types_are_cached() {
        assert_eq!(extension_for_content_type("image/png"), Some("png"));
        assert_eq!(
            extension_for_content_type("image/x-icon; charset=binary"),
            Some("ico")
        );
        assert_eq!(extension_for_content_type("IMAGE/JPEG"), Some("jpg"));
        // The soft-404 case that made the tile look broken.
        assert_eq!(extension_for_content_type("text/html; charset=UTF-8"), None);
        assert_eq!(extension_for_content_type(""), None);
    }

    #[test]
    fn redirects_preserve_paths_and_upgrade_http_without_sending_credentials() {
        let source = reqwest::Url::parse("https://example.com/icons/touch.png").expect("URL");
        for (location, expected) in [
            ("../favicon.ico", "https://example.com/favicon.ico"),
            ("//cdn.example/icon.png", "https://cdn.example/icon.png"),
            ("http://www.bild.de/", "https://www.bild.de/"),
            (
                "https://example.com/icon?v=2",
                "https://example.com/icon?v=2",
            ),
        ] {
            assert_eq!(
                https_redirect(&source, location)
                    .expect("redirect")
                    .as_str(),
                expected
            );
        }
        for location in [
            "file:///etc/passwd",
            "data:image/png;base64,aA==",
            "https://user:password@example.com/icon.png",
            "https://user@example.com/icon.png",
        ] {
            assert!(https_redirect(&source, location).is_err(), "{location}");
        }
    }

    #[test]
    fn an_atomic_write_leaves_no_partial_file_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("example.com.png");
        write_atomically(&target, b"bytes").expect("write icon");
        assert_eq!(std::fs::read(&target).expect("read icon"), b"bytes");
        assert!(!target.with_extension("part").exists());
    }
}
