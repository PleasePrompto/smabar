//! Native pointer backstop for hover state after the DOM event stream stops.

use std::time::Duration;

use anyhow::Context;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

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
            let sample = match crate::platform::bar_pointer_sample(&bar).await {
                Ok(sample) => {
                    if degraded {
                        tracing::info!("native pointer watchdog recovered");
                        degraded = false;
                    }
                    sample
                }
                Err(error) => {
                    if !degraded {
                        tracing::warn!(%error, "native pointer watchdog failed; hover cleanup will retry automatically");
                        degraded = true;
                    }
                    None
                }
            };
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
        tauri::async_runtime::spawn(async move {
            let sample = crate::platform::bar_pointer_sample(&bar).await.unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to resample the bar pointer; treating it as outside the bar");
                None
            });
            if let Err(error) = bar.emit("bar-pointer-sample", sample) {
                tracing::warn!(%error, "failed to report the bar pointer after a surface change");
            }
        });
        Ok(())
    }
}
