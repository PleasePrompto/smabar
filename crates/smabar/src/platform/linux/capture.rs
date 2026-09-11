//! Snapshots the bar's own webview.
//!
//! WebKit renders the DOM into a cairo surface, so the picture contains the
//! bar and nothing else — no desktop, no other windows. That is a property
//! of the source, not a filter applied afterwards.
//!
//! `FullDocument` rather than `Visible` on purpose: with the shell's capture
//! stage lifting `overflow`, the document grows past the viewport and a long
//! flyout is captured whole instead of at its scroll edge.

use gtk::cairo;
use smabar_core::capture::BarError;
use smabar_core::platform::Rect;
use tauri::WebviewWindow;
use tokio::sync::oneshot;
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

use super::super::capture_image::{device_crop, scaled_size};

/// PNG of `rect` (CSS pixels) out of the live webview, magnified by `scale`.
/// Returns the bytes and the image's pixel size.
pub async fn snapshot_png(
    window: &WebviewWindow,
    rect: Rect,
    scale_factor: f64,
    scale: u8,
) -> Result<(Vec<u8>, u32, u32), BarError> {
    let (tx, rx) = oneshot::channel();
    window
        .with_webview(move |platform| {
            platform.inner().snapshot(
                SnapshotRegion::FullDocument,
                SnapshotOptions::TRANSPARENT_BACKGROUND,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    let encoded = result
                        .map_err(|error| BarError::Failed(format!("webkit snapshot: {error}")))
                        .and_then(|surface| encode(&surface, rect, scale_factor, scale));
                    let _ = tx.send(encoded);
                },
            );
        })
        .map_err(|error| BarError::Failed(format!("cannot reach the webview: {error}")))?;
    rx.await.unwrap_or(Err(BarError::Disconnected))
}

/// Crops the snapshot to the subject and scales it into a PNG.
fn encode(
    surface: &cairo::Surface,
    rect: Rect,
    scale_factor: f64,
    scale: u8,
) -> Result<(Vec<u8>, u32, u32), BarError> {
    let source = cairo::ImageSurface::try_from(surface.clone())
        .map_err(|_| BarError::Failed("webkit returned a surface with no pixels".into()))?;
    let (x, y, width, height) = device_crop(rect, scale_factor, (source.width(), source.height()))
        .ok_or_else(|| {
            BarError::Failed(
                "the subject sits outside the rendered document — it is probably not visible"
                    .into(),
            )
        })?;
    let (out_w, out_h) = scaled_size(width, height, scale)?;

    let target = cairo::ImageSurface::create(cairo::Format::ARgb32, out_w, out_h)
        .map_err(|error| BarError::Failed(format!("cannot allocate the capture: {error}")))?;
    {
        let context = cairo::Context::new(&target)
            .map_err(|error| BarError::Failed(format!("cannot draw the capture: {error}")))?;
        let factor = f64::from(out_w) / f64::from(width);
        context.scale(factor, factor);
        context
            .set_source_surface(&source, f64::from(-x), f64::from(-y))
            .map_err(|error| BarError::Failed(format!("cannot place the capture: {error}")))?;
        // Nearest keeps a magnified tile's edges crisp; smoothing a 3x
        // blow-up of 12px text only makes it harder to read.
        context.source().set_filter(cairo::Filter::Nearest);
        context
            .paint()
            .map_err(|error| BarError::Failed(format!("cannot paint the capture: {error}")))?;
    }

    let mut png = Vec::new();
    target
        .write_to_png(&mut png)
        .map_err(|error| BarError::Failed(format!("cannot encode the PNG: {error}")))?;
    let width = u32::try_from(out_w).unwrap_or(0);
    let height = u32::try_from(out_h).unwrap_or(0);
    Ok((png, width, height))
}
