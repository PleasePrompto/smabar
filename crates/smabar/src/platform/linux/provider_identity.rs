use std::time::Duration;

use anyhow::Context;
use tauri::WebviewWindow;
use webkit2gtk::{UserMessage, WebContextExt, WebViewExt, glib::ToVariant};

const ORIGIN_MESSAGE: &str = "smabar-provider-origin";

pub(super) fn send(window: &WebviewWindow, origin: &str) -> anyhow::Result<()> {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let origin = origin.to_owned();
    window
        .with_webview(move |platform| {
            let result = platform
                .inner()
                .context()
                .ok_or_else(|| "WebKit returned no web context".to_string())
                .map(|context| {
                    let parameters = origin.to_variant();
                    // The broadcast reaches the current WebProcess; the
                    // initialization data covers a replacement after a crash.
                    context.set_web_extensions_initialization_user_data(&parameters);
                    context.send_message_to_all_extensions(&UserMessage::new(
                        ORIGIN_MESSAGE,
                        Some(&parameters),
                    ));
                });
            let _ = send.send(result);
        })
        .context("failed to reach WebKit for provider identity setup")?;
    receive
        .recv_timeout(Duration::from_secs(2))
        .context("timed out waiting for WebKit provider identity setup on the window thread")?
        .map_err(anyhow::Error::msg)
}
