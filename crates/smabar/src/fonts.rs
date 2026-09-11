//! Secure Google WOFF2 provisioning at the Tauri/network boundary.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Serialize;
use smabar_core::fonts::{self, FontOption, FontSource};
use tauri::State;

use crate::commands::AppState;

mod cache;
mod css;

use crate::http_util::{content_type_is, read_limited};
#[cfg(test)]
use cache::safe_cache_file;
use cache::{CachedFace, CachedFile, CachedManifest, load_cached, sha256, validate_woff2};
use css::{parse_css_faces, validate_weight};

const CSS2_ENDPOINT: &str = "https://fonts.googleapis.com/css2";
const CSS_HOST: &str = "fonts.googleapis.com";
const FILE_HOST: &str = "fonts.gstatic.com";
const USER_AGENT: &str =
    "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36";
const MAX_CSS_BYTES: usize = 1024 * 1024;
const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 96 * 1024 * 1024;
const MAX_FACE_DESCRIPTORS: usize = 1024;
const MAX_FONT_FILES: usize = 512;
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// ponytail: one global lock is enough for rare interactive installs; use
// per-family locks only if concurrent font downloads become measurable.
static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Lists installed system families and the complete checked-in Google catalog.
#[tauri::command]
pub async fn font_list(
    state: State<'_, AppState>,
    query: Option<String>,
    source: Option<FontSource>,
    monospaced: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<FontOption>, String> {
    let paths = state.paths().clone();
    tauri::async_runtime::spawn_blocking(move || {
        fonts::font_options(&paths, query.as_deref(), source, monospaced, limit)
    })
    .await
    .map_err(|error| format!("Could not scan installed fonts: {error}"))
}

/// A locally cached face ready for `new FontFace(...)` in the shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFaceDescriptor {
    family: String,
    path: String,
    style: String,
    weight: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unicode_range: Option<String>,
}

/// Result of ensuring one allowlisted Google family is available locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGoogleFont {
    id: String,
    family: String,
    faces: Vec<FontFaceDescriptor>,
}

/// Downloads only a catalog ID. Themes never provide a URL or filesystem path.
#[tauri::command]
pub async fn ensure_google_font(
    state: State<'_, AppState>,
    id: String,
) -> Result<InstalledGoogleFont, String> {
    let Some(font) = fonts::google_font(&id) else {
        return Err(format!(
            "Unknown Google font {id:?}; choose a family returned by font_list."
        ));
    };
    let family = font.family().to_string();
    let directory = state.paths().google_fonts_dir();
    let _guard = INSTALL_LOCK.lock().await;
    match ensure_at(&directory, font).await {
        Ok(installed) => Ok(installed),
        Err(error) => {
            tracing::warn!(font_id = %id, %family, error = %format!("{error:#}"), "Google font provisioning failed; keeping the CSS fallback");
            Err(format!(
                "Could not download {family}. The fallback font remains active; check the network and try again."
            ))
        }
    }
}

async fn ensure_at(
    google_fonts_dir: &Path,
    font: &'static fonts::GoogleFont,
) -> anyhow::Result<InstalledGoogleFont> {
    let family_dir = google_fonts_dir.join(font.id());
    if let Some(cached) = load_cached(&family_dir, font.id(), font.family()) {
        return Ok(cached);
    }
    remove_invalid_cache(&family_dir)?;
    std::fs::create_dir_all(&family_dir)
        .with_context(|| format!("cannot create font cache {}", family_dir.display()))?;

    let client = http_client()?;
    let spec = font.css2_family_spec();
    let mut css_url = reqwest::Url::parse(CSS2_ENDPOINT).context("CSS2 endpoint is invalid")?;
    css_url
        .query_pairs_mut()
        .append_pair("family", &spec)
        .append_pair("display", "swap");
    let response = client
        .get(css_url)
        .send()
        .await
        .context("Google Fonts CSS request failed")?;
    validate_response_url(response.url(), CSS_HOST)?;
    if !response.status().is_success() {
        bail!("Google Fonts CSS returned {}", response.status());
    }
    if !content_type_is(response.headers(), "text/css") {
        bail!("Google Fonts CSS response had an unexpected content type");
    }
    let css = read_limited(response, MAX_CSS_BYTES, "font stylesheet").await?;
    let css = std::str::from_utf8(&css).context("font stylesheet is not UTF-8")?;
    let remote_faces = parse_css_faces(css, font.family())?;

    let mut urls = BTreeMap::<String, String>::new();
    for face in &remote_faces {
        let next_index = urls.len();
        urls.entry(face.url.to_string())
            .or_insert_with(|| format!("font-{next_index:03}.woff2"));
    }
    if urls.len() > MAX_FONT_FILES {
        bail!("font stylesheet referenced too many files");
    }

    let mut total_bytes = 0_usize;
    let mut files = BTreeMap::new();
    for (url, file) in &urls {
        let url = reqwest::Url::parse(url).context("catalog font URL is invalid")?;
        validate_response_url(&url, FILE_HOST)?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .with_context(|| format!("font file request to {FILE_HOST} failed"))?;
        validate_response_url(response.url(), FILE_HOST)?;
        if !response.status().is_success() {
            bail!("font file returned {}", response.status());
        }
        if !woff2_content_type(response.headers()) {
            bail!("font file had an unexpected content type");
        }
        let bytes = read_limited(response, MAX_FONT_BYTES, "font file").await?;
        validate_woff2(&bytes)?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .context("font download size overflow")?;
        if total_bytes > MAX_TOTAL_BYTES {
            bail!("font family exceeded the download size limit");
        }
        crate::icons::write_atomically(&family_dir.join(file), &bytes)
            .with_context(|| format!("cannot write cached font file {file}"))?;
        files.insert(
            file.clone(),
            CachedFile {
                bytes: bytes.len(),
                sha256: sha256(&bytes),
            },
        );
    }

    let faces = remote_faces
        .into_iter()
        .map(|face| CachedFace {
            file: urls[face.url.as_str()].clone(),
            style: face.style,
            weight: face.weight,
            unicode_range: face.unicode_range,
        })
        .collect();
    let manifest = CachedManifest {
        version: 1,
        id: font.id().to_string(),
        family: font.family().to_string(),
        faces,
        files,
    };
    let mut json = serde_json::to_vec_pretty(&manifest).context("cannot encode font manifest")?;
    json.push(b'\n');
    crate::icons::write_atomically(&family_dir.join("manifest.json"), &json)
        .context("cannot write font manifest")?;
    manifest.into_installed(&family_dir)
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    let redirects = reqwest::redirect::Policy::custom(|attempt| {
        let allowed = attempt
            .url()
            .host_str()
            .is_some_and(|host| matches!(host, CSS_HOST | FILE_HOST));
        if attempt.previous().len() >= 3 || !allowed {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    reqwest::Client::builder()
        .https_only(true)
        .redirect(redirects)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .context("cannot build Google Fonts HTTP client")
}

fn woff2_content_type(headers: &reqwest::header::HeaderMap) -> bool {
    [
        "font/woff2",
        "application/font-woff2",
        "application/x-font-woff2",
        "application/octet-stream",
    ]
    .iter()
    .any(|expected| content_type_is(headers, expected))
}

fn remove_invalid_cache(directory: &Path) -> anyhow::Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(directory) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(directory)
    } else {
        std::fs::remove_file(directory)
    }
    .with_context(|| format!("cannot replace invalid font cache {}", directory.display()))
}

fn validate_response_url(url: &reqwest::Url, expected_host: &str) -> anyhow::Result<()> {
    if url.scheme() != "https"
        || url.host_str() != Some(expected_host)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        bail!("font request left the allowlisted Google host");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
