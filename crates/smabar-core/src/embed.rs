//! Isolated HTTP origin for third-party media players.
//!
//! Tauri serves the production shell from a privileged custom origin. The
//! shell therefore frames this static loopback page, and this page frames the
//! provider: the provider stays outside every Tauri remote capability and
//! receives a normal HTTP origin. Native WebView hooks replace that wrapper
//! origin with the registered desktop application identity on the provider's
//! first document request.

use std::net::{Ipv4Addr, SocketAddr};

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// Capabilities required by normal media players, without device access or
/// top-level navigation.
pub const EMBED_ALLOW: &str = "autoplay; encrypted-media; fullscreen; picture-in-picture";
pub const EMBED_SANDBOX: &str = "allow-same-origin allow-scripts";
pub const EMBED_REFERRER_POLICY: &str = "strict-origin-when-cross-origin";
/// HTTPS representation of Tauri's registered reverse-DNS application id.
pub const APP_IDENTITY: &str = "https://dev.smabar.desktop/";

const WRAPPER_CSP: &str = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; frame-src https:; object-src 'none'; base-uri 'none'; form-action 'none'";
const MAX_TITLE_UTF16_UNITS: usize = 512;
const MAX_SOURCE_UTF16_UNITS: usize = 8192;

/// Failure to start the private loopback endpoint.
#[derive(Debug, Error)]
pub enum EmbedServeError {
    #[error("cannot create the private media embed path: {0}")]
    Random(#[source] getrandom::Error),
    #[error("cannot start the media embed server on 127.0.0.1: {0}")]
    Bind(#[source] std::io::Error),
}

/// Running loopback wrapper. Its random port and unlisted path are handed to
/// the shell once in its initial UI state.
pub struct EmbedServer {
    addr: SocketAddr,
    path: String,
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl EmbedServer {
    /// Origin the native WebView hook recognizes as the isolated wrapper.
    pub fn origin(&self) -> String {
        format!("http://{}/", self.addr)
    }

    /// URL the shell may frame. Provider URLs are validated by the wrapper.
    pub fn url(&self) -> String {
        format!("http://{}{}", self.addr, self.path)
    }

    /// Graceful stop used by the transport test.
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take()
            && let Err(error) = task.await
        {
            tracing::warn!(%error, "media embed server task ended abnormally; restart smabar to restore remote players");
        }
    }
}

impl Drop for EmbedServer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Starts the isolated wrapper on an operating-system-selected loopback port.
pub async fn serve() -> Result<EmbedServer, EmbedServeError> {
    let mut token = [0_u8; 24];
    getrandom::fill(&mut token).map_err(EmbedServeError::Random)?;
    let path = format!("/{}/embed", URL_SAFE_NO_PAD.encode(token));
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(EmbedServeError::Bind)?;
    let addr = listener.local_addr().map_err(EmbedServeError::Bind)?;
    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    let router = axum::Router::new()
        .route(&path, get(wrapper))
        .with_state(addr.to_string());
    let task = tokio::spawn(async move {
        let served = axum::serve(listener, router)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await;
        if let Err(error) = served {
            tracing::error!(%error, "remote players are unavailable until smabar restarts; local img, audio and video remain supported");
        }
    });
    Ok(EmbedServer {
        addr,
        path,
        cancel,
        task: Some(task),
    })
}

async fn wrapper(
    State(expected_host): State<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        != Some(expected_host.as_str())
    {
        return with_security_headers(StatusCode::BAD_REQUEST.into_response());
    }
    let mut source = None;
    let mut title = None;
    for (name, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match name.as_ref() {
            "src" if source.is_none() => source = Some(value.into_owned()),
            "title" if title.is_none() => title = Some(value.into_owned()),
            _ => {}
        }
    }
    let Some((source, title)) = valid_embed(source.as_deref(), title.as_deref()) else {
        return with_security_headers(StatusCode::BAD_REQUEST.into_response());
    };
    let source = escape_attribute(source.as_str());
    let title = escape_attribute(title);
    let html = format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="referrer" content="{EMBED_REFERRER_POLICY}"><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body,iframe{{box-sizing:border-box;width:100%;height:100%}}html,body{{margin:0;overflow:hidden;background:transparent}}iframe{{display:block;border:0}}</style></head>
<body><iframe src="{source}" title="{title}" allow="{EMBED_ALLOW}" allowfullscreen referrerpolicy="{EMBED_REFERRER_POLICY}" sandbox="{EMBED_SANDBOX}"></iframe></body></html>"#,
    );
    with_security_headers(Html(html).into_response())
}

fn valid_embed<'a>(source: Option<&str>, title: Option<&'a str>) -> Option<(url::Url, &'a str)> {
    let title = title?.trim();
    if title.is_empty() || title.encode_utf16().count() > MAX_TITLE_UTF16_UNITS {
        return None;
    }
    let source = url::Url::parse(source?).ok()?;
    (source.as_str().encode_utf16().count() <= MAX_SOURCE_UTF16_UNITS
        && source.scheme() == "https"
        && source.username().is_empty()
        && source.password().is_none())
    .then_some((source, title))
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn with_security_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(WRAPPER_CSP),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static(EMBED_REFERRER_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn serves_only_the_isolated_https_frame_wrapper() {
        let server = serve().await.expect("start embed server");
        let mut stream = tokio::net::TcpStream::connect(server.addr)
            .await
            .expect("connect");
        let request = format!(
            "GET {}?src=https%3A%2F%2Fwww.youtube-nocookie.com%2Fembed%2Fabc%3Fstart%3D5&title=Latest+video HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            server.path, server.addr
        );
        stream.write_all(request.as_bytes()).await.expect("request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("response");

        let (headers, body) = response
            .split_once("\r\n\r\n")
            .expect("HTTP response has a header/body boundary");
        assert!(headers.starts_with("HTTP/1.1 200 OK"), "{response}");
        for expected in [
            format!("content-security-policy: {WRAPPER_CSP}"),
            format!("referrer-policy: {EMBED_REFERRER_POLICY}"),
            "x-content-type-options: nosniff".to_owned(),
            "cache-control: no-store".to_owned(),
            "content-type: text/html; charset=utf-8".to_owned(),
        ] {
            assert!(
                headers
                    .lines()
                    .any(|line| line.eq_ignore_ascii_case(&expected)),
                "missing header {expected:?} in {headers:?}"
            );
        }
        assert!(body.contains(EMBED_ALLOW), "{body}");
        assert!(body.contains(EMBED_SANDBOX), "{body}");
        assert!(body.contains(EMBED_REFERRER_POLICY), "{body}");
        assert!(
            body.contains("https://www.youtube-nocookie.com/embed/abc?start=5"),
            "{body}"
        );
        assert!(!body.contains("<script"), "{body}");
        server.shutdown().await;
    }

    #[tokio::test]
    async fn hides_the_wrapper_behind_a_fresh_unlisted_path() {
        let first = serve().await.expect("start first embed server");
        let second = serve().await.expect("start second embed server");
        assert_ne!(first.path, second.path);
        assert!(first.path.starts_with('/'));
        assert!(first.path.ends_with("/embed"));
        assert_eq!(first.path.len(), 1 + 32 + "/embed".len());

        let mut stream = tokio::net::TcpStream::connect(first.addr)
            .await
            .expect("connect");
        let request = format!(
            "GET /embed?src=https%3A%2F%2Fexample.com&title=Video HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            first.addr
        );
        stream.write_all(request.as_bytes()).await.expect("request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("response");
        assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");

        first.shutdown().await;
        second.shutdown().await;
    }

    #[tokio::test]
    async fn rejects_requests_for_a_rebound_host() {
        let server = serve().await.expect("start embed server");
        let mut stream = tokio::net::TcpStream::connect(server.addr)
            .await
            .expect("connect");
        stream
            .write_all(
                format!(
                    "GET {}?src=https%3A%2F%2Fexample.com&title=Video HTTP/1.1\r\nHost: attacker.example\r\nConnection: close\r\n\r\n",
                    server.path
                )
                .as_bytes(),
            )
            .await
            .expect("request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("response");
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "{response}"
        );
        server.shutdown().await;
    }

    #[test]
    fn validates_and_escapes_embed_inputs() {
        assert!(valid_embed(Some("http://example.com"), Some("Video")).is_none());
        assert!(valid_embed(Some("https://user:secret@example.com"), Some("Video")).is_none());
        assert!(valid_embed(Some("https://example.com"), Some(" ")).is_none());
        assert!(valid_embed(Some("https://example.com"), Some(&"x".repeat(513))).is_none());
        assert!(
            valid_embed(
                Some(&format!("https://example.com/{}", "x".repeat(8193))),
                Some("Video")
            )
            .is_none()
        );
        assert_eq!(
            escape_attribute("<video title=\"x\">&'"),
            "&lt;video title=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn preserves_provider_urls_without_a_core_allowlist_or_rewrite() {
        for source in [
            "https://www.youtube.com/embed/abc?start=5&tile_referrer=https://example.test/",
            "https://www.youtube-nocookie.com/embed/abc?autoplay=1",
            "https://player.vimeo.com/video/123?autoplay=1",
            "https://media.example.test/player?id=123&mode=compact",
        ] {
            let (url, _) = valid_embed(Some(source), Some("Video")).expect("valid embed");
            assert_eq!(url.as_str(), source);
        }
    }

    #[test]
    fn application_identity_matches_the_tauri_identifier() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../smabar/tauri.conf.json"))
                .expect("Tauri config");
        let identifier = config["identifier"].as_str().expect("Tauri identifier");
        assert_eq!(APP_IDENTITY, format!("https://{identifier}/"));
    }

    #[test]
    fn wrapper_policy_matches_the_public_ui_contract() {
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui-kit/contract.json"))
                .expect("ui-kit contract");
        assert_eq!(contract["media"]["embedAllow"], EMBED_ALLOW);
        assert_eq!(contract["media"]["embedSandbox"], EMBED_SANDBOX);
        assert_eq!(
            contract["media"]["embedReferrerPolicy"],
            EMBED_REFERRER_POLICY
        );
        assert_eq!(contract["media"]["appIdentity"], APP_IDENTITY);
    }
}
