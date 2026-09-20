//! The Community Store's network edge: reqwest behind the core's
//! [`StoreFetcher`] seam, the endpoint, and the development overrides.
//!
//! Store requests never follow redirects (the store issues none); downloads
//! from GitHub may hop to its content hosts and nowhere else.

use smabar_core::config::{ConfigWatcher, SmabarPaths};
use smabar_core::plugins::PluginSupervisor;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use minisign_verify::PublicKey;
use reqwest::Url;
use smabar_core::store::fetch::{
    FetchBody, FetchError, FetchFuture, FetchOutcome, ProgressFn, StoreFetcher,
};

pub const STORE_ENDPOINT: &str = "https://store.smabar.com/";
/// Where GitHub sends archive and raw-file requests.
const DOWNLOAD_HOSTS: &[&str] = &[
    "github.com",
    "codeload.github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
];
const REDIRECT_LIMIT: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The store base URL: `SMABAR_STORE_ENDPOINT` when set (dev.sh may point it
/// at a local store), else the real one. Plain http is accepted in debug
/// builds only.
pub fn store_endpoint() -> Result<Url, String> {
    parse_endpoint(
        std::env::var("SMABAR_STORE_ENDPOINT").ok().as_deref(),
        cfg!(debug_assertions),
    )
}

fn parse_endpoint(value: Option<&str>, allow_http: bool) -> Result<Url, String> {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(STORE_ENDPOINT);
    let mut url: Url = raw
        .parse()
        .map_err(|error| format!("SMABAR_STORE_ENDPOINT is not a URL: {error}"))?;
    match url.scheme() {
        "https" => {}
        "http" if allow_http => {}
        scheme => {
            return Err(format!(
                "SMABAR_STORE_ENDPOINT must use https (got {scheme}); plain http is for debug builds"
            ));
        }
    }
    if url.host_str().is_none() {
        return Err("SMABAR_STORE_ENDPOINT has no host".to_string());
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// The catalog key: the embedded one, or `SMABAR_STORE_PUBKEY` in a debug
/// build so a local store started with its own `keygen` can be tested.
/// Release builds ignore the variable.
pub fn store_public_key() -> Result<PublicKey, String> {
    if cfg!(debug_assertions)
        && let Some(line) = std::env::var("SMABAR_STORE_PUBKEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
    {
        tracing::warn!(
            "using the SMABAR_STORE_PUBKEY override for catalog signatures (debug build)"
        );
        return PublicKey::from_base64(line.trim())
            .map_err(|error| format!("SMABAR_STORE_PUBKEY is not a minisign public key: {error}"));
    }
    smabar_core::store::catalog::embedded_key().map_err(|error| error.to_string())
}

/// reqwest as the core's [`StoreFetcher`].
pub struct ReqwestFetcher {
    store: reqwest::Client,
    download: reqwest::Client,
    endpoint_host: String,
}

impl ReqwestFetcher {
    pub fn new(endpoint: &Url) -> anyhow::Result<Self> {
        let endpoint_host = endpoint.host_str().unwrap_or_default().to_string();
        let https_only = endpoint.scheme() == "https";
        let user_agent = format!("smabar/{}", env!("CARGO_PKG_VERSION"));
        let store = reqwest::Client::builder()
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(&user_agent)
            .build()
            .context("cannot build the store HTTP client")?;
        let redirects = reqwest::redirect::Policy::custom(|attempt| {
            let allowed = attempt
                .url()
                .host_str()
                .is_some_and(|host| DOWNLOAD_HOSTS.contains(&host));
            if attempt.previous().len() >= REDIRECT_LIMIT || !allowed {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let download = reqwest::Client::builder()
            .https_only(true)
            .redirect(redirects)
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(DOWNLOAD_READ_TIMEOUT)
            .timeout(DOWNLOAD_TIMEOUT)
            .user_agent(&user_agent)
            .build()
            .context("cannot build the download HTTP client")?;
        Ok(Self {
            store,
            download,
            endpoint_host,
        })
    }

    fn client_for(&self, url: &Url) -> Result<&reqwest::Client, FetchError> {
        let host = url.host_str().unwrap_or_default();
        if host == self.endpoint_host {
            return Ok(&self.store);
        }
        if DOWNLOAD_HOSTS.contains(&host) {
            return Ok(&self.download);
        }
        Err(FetchError::Other(format!(
            "{url} is not on the store or a GitHub content host; refusing the request"
        )))
    }
}

fn classify(error: reqwest::Error, url: &Url) -> FetchError {
    let host = url.host_str().unwrap_or_default().to_string();
    if error.is_connect() || error.is_timeout() || error.is_request() {
        FetchError::Offline {
            host,
            reason: error.without_url().to_string(),
        }
    } else {
        FetchError::Other(error.without_url().to_string())
    }
}

impl StoreFetcher for ReqwestFetcher {
    fn get<'a>(
        &'a self,
        url: &'a Url,
        if_none_match: Option<&'a str>,
        max_bytes: usize,
    ) -> FetchFuture<'a, FetchOutcome> {
        Box::pin(async move {
            let client = self.client_for(url)?;
            let mut request = client.get(url.clone());
            if let Some(etag) = if_none_match {
                request = request.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            let response = request.send().await.map_err(|error| classify(error, url))?;
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(FetchOutcome::NotModified);
            }
            if !response.status().is_success() {
                return Err(FetchError::Status {
                    url: url.to_string(),
                    status: response.status().as_u16(),
                });
            }
            let etag = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let bytes = crate::http_util::read_limited(response, max_bytes, "store response")
                .await
                .map_err(|error| {
                    FetchError::TooLarge {
                        what: "the response",
                        limit: max_bytes as u64,
                    }
                    .into_other_unless_size(error)
                })?;
            Ok(FetchOutcome::Body(FetchBody { bytes, etag }))
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a Url,
        target: &'a Path,
        max_bytes: u64,
        progress: ProgressFn<'a>,
    ) -> FetchFuture<'a, u64> {
        Box::pin(async move {
            let client = self.client_for(url)?;
            let mut response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|error| classify(error, url))?;
            if !response.status().is_success() {
                return Err(FetchError::Status {
                    url: url.to_string(),
                    status: response.status().as_u16(),
                });
            }
            let total = response.content_length();
            if total.is_some_and(|total| total > max_bytes) {
                return Err(FetchError::TooLarge {
                    what: "the archive",
                    limit: max_bytes,
                });
            }
            let partial = target.with_extension("part");
            let io = |source| FetchError::Io {
                path: partial.clone(),
                source,
            };
            let mut file = tokio::fs::File::create(&partial).await.map_err(io)?;
            let mut received = 0u64;
            let result: Result<(), FetchError> = async {
                use tokio::io::AsyncWriteExt as _;
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|error| classify(error, url))?
                {
                    received = received.saturating_add(chunk.len() as u64);
                    if received > max_bytes {
                        return Err(FetchError::TooLarge {
                            what: "the archive",
                            limit: max_bytes,
                        });
                    }
                    file.write_all(&chunk).await.map_err(io)?;
                    progress(received, total);
                }
                file.flush().await.map_err(io)
            }
            .await;
            drop(file);
            if let Err(error) = result {
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(error);
            }
            tokio::fs::rename(&partial, target)
                .await
                .map_err(|source| FetchError::Io {
                    path: target.to_path_buf(),
                    source,
                })?;
            Ok(received)
        })
    }
}

trait UnlessSize {
    fn into_other_unless_size(self, error: anyhow::Error) -> FetchError;
}

impl UnlessSize for FetchError {
    /// `read_limited` reports every failure as one error type; only the size
    /// limit is the caller's own rule, everything else is a read failure.
    fn into_other_unless_size(self, error: anyhow::Error) -> FetchError {
        let text = error.to_string();
        if text.contains("size limit") {
            self
        } else {
            FetchError::Other(text)
        }
    }
}

/// The Community Store client over the app's HTTP edge. The endpoint and
/// the key are the app edge's decisions (environment overrides live here).
pub fn build_store(
    paths: &SmabarPaths,
    watcher: &Arc<ConfigWatcher>,
    supervisor: &PluginSupervisor,
    reserved_ids: std::collections::BTreeSet<String>,
) -> anyhow::Result<smabar_core::store::StoreService> {
    let endpoint = store_endpoint().map_err(anyhow::Error::msg)?;
    let key = store_public_key().map_err(anyhow::Error::msg)?;
    let fetcher = Arc::new(ReqwestFetcher::new(&endpoint)?);
    smabar_core::store::StoreService::new(
        paths.clone(),
        Arc::clone(watcher),
        supervisor.clone(),
        fetcher,
        smabar_core::store::StoreOptions {
            endpoint,
            key,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            reserved_ids,
        },
    )
    .context("cannot start the Community Store client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_endpoint_is_the_real_store() {
        assert_eq!(
            parse_endpoint(None, false).expect("default").as_str(),
            STORE_ENDPOINT
        );
        assert_eq!(
            parse_endpoint(Some("  "), false).expect("blank").as_str(),
            STORE_ENDPOINT
        );
    }

    #[test]
    fn overrides_are_normalized_and_http_is_debug_only() {
        let url = parse_endpoint(Some("http://127.0.0.1:8787/store?x=1"), true).expect("debug");
        assert_eq!(url.as_str(), "http://127.0.0.1:8787/store/");
        let error = parse_endpoint(Some("http://127.0.0.1:8787/"), false).expect_err("release");
        assert!(error.contains("https"));
        assert!(parse_endpoint(Some("not a url"), true).is_err());
    }
}
