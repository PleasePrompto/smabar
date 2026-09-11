//! Captures the current WebView2 viewport without photographing the desktop.

use std::time::Duration;

use smabar_core::capture::BarError;
use smabar_core::platform::Rect;
use tauri::WebviewWindow;
use tokio::sync::mpsc;
use webview2_com::{
    CapturePreviewCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, ICoreWebView2Controller,
    },
};
use windows_webview2::Win32::{
    System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
    UI::Shell::SHCreateMemStream,
};

use super::super::capture_image::{MAX_DECODE_PIXELS, crop_scale_png, zeroed};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(4);
const READ_BUFFER_SIZE: usize = 16 * 1024;
// RGBA pixels plus ample PNG framing/compression overhead. This bounds our
// own copy of the stream before allocating it.
const MAX_PNG_BYTES: u64 = MAX_DECODE_PIXELS * 5;

/// PNG of `rect` (CSS pixels) out of the visible WebView2 viewport.
pub async fn snapshot_png(
    window: &WebviewWindow,
    rect: Rect,
    scale_factor: f64,
    scale: u8,
) -> Result<(Vec<u8>, u32, u32), BarError> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    window
        .with_webview(move |platform| {
            if tx.is_closed() {
                return;
            }
            if let Err(error) = begin_capture(platform.controller(), tx.clone()) {
                let _ = tx.send(Err(error));
            }
        })
        .map_err(|error| BarError::Failed(format!("cannot reach the webview: {error}")))?;

    let bytes = tokio::time::timeout(CAPTURE_TIMEOUT, rx.recv())
        .await
        .map_err(|_| BarError::Timeout(CAPTURE_TIMEOUT))?
        .ok_or(BarError::Disconnected)??;
    tokio::task::spawn_blocking(move || crop_scale_png(&bytes, rect, scale_factor, scale))
        .await
        .map_err(|error| BarError::Failed(format!("the capture worker failed: {error}")))?
}

/// Starts WebView2's async capture on the UI thread. The completion handler
/// only copies the memory stream and sends it away; PNG work happens after it.
fn begin_capture(
    controller: ICoreWebView2Controller,
    tx: mpsc::UnboundedSender<Result<Vec<u8>, BarError>>,
) -> Result<(), BarError> {
    let stream = unsafe { SHCreateMemStream(None) }
        .ok_or_else(|| BarError::Failed("cannot allocate the WebView2 capture stream".into()))?;
    let webview = unsafe { controller.CoreWebView2() }
        .map_err(|error| BarError::Failed(format!("cannot access WebView2: {error}")))?;
    let callback_stream = stream.clone();
    let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
        let capture = result
            .map_err(|error| BarError::Failed(format!("WebView2 capture failed: {error}")))
            // Keep the UI callback to one bounded in-memory copy. PNG decode,
            // crop, scale and encode run on a blocking worker afterwards.
            .and_then(|()| read_stream(&callback_stream));
        let _ = tx.send(capture);
        Ok(())
    }));
    unsafe {
        webview.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
    }
    .map_err(|error| BarError::Failed(format!("cannot start WebView2 capture: {error}")))
}

fn read_stream(stream: &IStream) -> Result<Vec<u8>, BarError> {
    let mut stat = STATSTG::default();
    unsafe { stream.Stat(&mut stat, STATFLAG_NONAME) }
        .map_err(|error| BarError::Failed(format!("cannot size WebView2 capture: {error}")))?;
    if stat.cbSize == 0 {
        return Err(BarError::Failed(
            "WebView2 returned an empty capture stream".into(),
        ));
    }
    if stat.cbSize > MAX_PNG_BYTES {
        return Err(BarError::Failed(format!(
            "the WebView2 PNG is {} bytes (limit {MAX_PNG_BYTES})",
            stat.cbSize
        )));
    }
    let length = usize::try_from(stat.cbSize)
        .map_err(|error| BarError::Failed(format!("the WebView2 PNG is too large: {error}")))?;
    let mut bytes = zeroed(length)?;

    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }
        .map_err(|error| BarError::Failed(format!("cannot rewind WebView2 capture: {error}")))?;
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        let chunk_len = remaining.min(READ_BUFFER_SIZE);
        let requested = u32::try_from(chunk_len)
            .map_err(|error| BarError::Failed(format!("invalid stream chunk size: {error}")))?;
        let mut read = 0_u32;
        unsafe {
            stream.Read(
                bytes[offset..].as_mut_ptr().cast(),
                requested,
                Some(&mut read),
            )
        }
        .ok()
        .map_err(|error| BarError::Failed(format!("cannot read WebView2 capture: {error}")))?;
        if read == 0 {
            return Err(BarError::Failed(format!(
                "WebView2 returned only {offset} of {} capture bytes",
                bytes.len()
            )));
        }
        let read = usize::try_from(read)
            .map_err(|error| BarError::Failed(format!("invalid stream read size: {error}")))?;
        if read > chunk_len {
            return Err(BarError::Failed(
                "WebView2 reported more stream bytes than requested".into(),
            ));
        }
        offset += read;
    }
    Ok(bytes)
}
