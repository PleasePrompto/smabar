//! Replaces the private wrapper origin on provider document requests.

use std::sync::Once;
use std::time::Duration;

use anyhow::Context;
use smabar_core::embed::APP_IDENTITY;
use tauri::WebviewWindow;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT, ICoreWebView2_22,
        ICoreWebView2Controller, ICoreWebView2WebResourceRequestedEventArgs,
    },
    WebResourceRequestedEventHandler, take_pwstr,
};
use windows_webview2::core::{BOOL, HSTRING, Interface, PWSTR};

static CALLBACK_ERROR: Once = Once::new();

pub fn install(window: &WebviewWindow, wrapper_origin: &str) -> anyhow::Result<()> {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let wrapper_origin = wrapper_origin.to_owned();
    window
        .with_webview(move |platform| {
            let result = install_on_controller(platform.controller(), wrapper_origin)
                .map_err(|error| error.to_string());
            let _ = send.send(result);
        })
        .context("failed to reach WebView2 for provider identity setup")?;
    receive
        .recv_timeout(Duration::from_secs(2))
        .context("timed out waiting for WebView2 provider identity setup on the window thread")?
        .map_err(anyhow::Error::msg)
}

fn install_on_controller(
    controller: ICoreWebView2Controller,
    wrapper_origin: String,
) -> windows_webview2::core::Result<()> {
    let webview = unsafe { controller.CoreWebView2()? };
    let version: ICoreWebView2_22 = webview.cast()?;
    unsafe {
        version.AddWebResourceRequestedFilterWithRequestSourceKinds(
            &HSTRING::from("https://*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT,
        )?;
    }

    let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
        if let Some(args) = args
            && let Err(error) = identify_request(&args, &wrapper_origin)
        {
            CALLBACK_ERROR.call_once(|| {
                tracing::warn!(
                    %error,
                    "failed to identify a provider document request; use its external fallback link and restart smabar"
                );
            });
        }
        Ok(())
    }));
    let mut token = 0;
    unsafe { webview.add_WebResourceRequested(&handler, &mut token) }
}

fn identify_request(
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    wrapper_origin: &str,
) -> windows_webview2::core::Result<()> {
    let request = unsafe { args.Request()? };
    let headers = unsafe { request.Headers()? };
    let name = HSTRING::from("Referer");
    let mut present = BOOL::default();
    unsafe { headers.Contains(&name, &mut present)? };
    if !present.as_bool() {
        return Ok(());
    }

    let mut value = PWSTR::null();
    unsafe { headers.GetHeader(&name, &mut value)? };
    if take_pwstr(value) == wrapper_origin {
        unsafe { headers.SetHeader(&name, &HSTRING::from(APP_IDENTITY))? };
    }
    Ok(())
}
