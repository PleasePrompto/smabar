//! Native pointer backstop for hover state after the DOM event stream stops.

use std::time::Duration;

use anyhow::Context;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use super::{SurfaceManager, SurfaceRole};

const WATCHDOG_INTERVAL: Duration = Duration::from_millis(150);

pub fn install_watchdog(app: &AppHandle) {
    if !crate::platform::needs_bar_pointer_watchdog() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut previous = None;
        let mut degraded = false;
        loop {
            tokio::time::sleep(WATCHDOG_INTERVAL).await;
            let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) else {
                return;
            };
            let sample = match bar_pointer_sample(&app, &bar) {
                Ok(sample) => sample,
                Err(error) => {
                    if !degraded {
                        tracing::warn!(%error, "native pointer watchdog failed; hover cleanup will retry automatically");
                        degraded = true;
                    }
                    continue;
                }
            };
            if degraded {
                tracing::info!("native pointer watchdog recovered");
                degraded = false;
            }
            if previous == Some(sample) {
                continue;
            }
            if let Err(error) = bar.emit("bar-pointer-sample", sample) {
                tracing::warn!(%error, "failed to report the native pointer sample; restart smabar to restore hover cleanup");
                return;
            }
            previous = Some(sample);
        }
    });
}

impl SurfaceManager {
    pub(super) fn trigger_origin(
        &self,
        window: &WebviewWindow,
    ) -> anyhow::Result<PhysicalPosition<i32>> {
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("surface lifecycle lock poisoned"))?;
        let frame =
            SurfaceRole::from_label(window.label()).and_then(|role| lifecycle.trigger_frame(role));
        drop(lifecycle);
        if let Some(frame) = frame {
            return Ok(PhysicalPosition::new(frame.x, frame.y));
        }
        window
            .outer_position()
            .context("surface has no managed or native position")
    }

    pub(super) fn report_bar_pointer(&self, app: &AppHandle) -> anyhow::Result<()> {
        let Some(bar) = app.get_webview_window(SurfaceRole::Bar.label()) else {
            return Ok(());
        };
        if crate::platform::report_native_bar_pointer(&bar)? {
            return Ok(());
        }
        let sample = bar_pointer_sample(app, &bar).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to sample the pointer after overlay focus loss; treating it as outside the bar");
            None
        });
        bar.emit("bar-pointer-sample", sample)
            .context("failed to report the pointer after overlay focus loss")
    }
}

fn bar_pointer_sample(app: &AppHandle, bar: &WebviewWindow) -> tauri::Result<Option<[f64; 2]>> {
    Ok(local_pointer_sample(
        app.cursor_position()?,
        bar.outer_position()?,
        bar.inner_size()?,
        bar.scale_factor()?,
    ))
}

pub(super) fn local_pointer_sample(
    cursor: PhysicalPosition<f64>,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> Option<[f64; 2]> {
    let x = cursor.x - f64::from(origin.x);
    let y = cursor.y - f64::from(origin.y);
    (scale.is_finite()
        && scale > 0.0
        && x >= 0.0
        && y >= 0.0
        && x < f64::from(size.width)
        && y < f64::from(size.height))
    .then_some([x / scale, y / scale])
}
