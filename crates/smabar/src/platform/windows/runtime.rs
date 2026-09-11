//! A missing Evergreen runtime must be explainable before a webview exists.

use anyhow::Context;
use smabar_core::{config::SmabarPaths, i18n};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::{HSTRING, w};

pub fn check(paths: &SmabarPaths, language: &str, minimum: Option<&str>) -> anyhow::Result<()> {
    let version = tauri::webview_version();
    let supported = version.as_ref().is_ok_and(|current| {
        parse_version(current).is_some_and(|current| {
            minimum.is_none_or(|minimum| parse_version(minimum).is_some_and(|min| current >= min))
        })
    });
    if supported {
        return Ok(());
    }
    tracing::error!(
        ?version,
        minimum,
        "WebView2 is missing or too old; install the Evergreen runtime from Microsoft and restart smabar"
    );
    let locale = i18n::resolve(paths, language);
    let message = locale
        .get("startup.webview2Required")
        .context("missing WebView2 startup translation")?;
    let text = HSTRING::from(message);
    // SAFETY: no owner window exists yet; both strings live through the modal call.
    unsafe {
        MessageBoxW(None, &text, w!("smabar"), MB_OK | MB_ICONERROR);
    }
    anyhow::bail!(
        "WebView2 is missing or too old; install Microsoft Edge WebView2 Evergreen and restart smabar"
    )
}

fn parse_version(value: &str) -> Option<[u32; 4]> {
    let parts: Vec<u32> = value
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    parts.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numeric_runtime_versions_and_rejects_preview_labels() {
        assert!(parse_version("122.0.2365.46") > parse_version("99.0.0.0"));
        assert_eq!(parse_version("122.0.2365.46 beta"), None);
        assert_eq!(parse_version("122.0"), None);
    }
}
