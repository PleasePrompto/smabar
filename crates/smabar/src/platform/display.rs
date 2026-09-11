//! Platform-neutral snapshots of the currently connected displays.

use anyhow::Context;
use serde::Serialize;
use sha2::{Digest, Sha256};
use smabar_core::platform::surfaces::ScreenRect;
use tauri::{AppHandle, Monitor};

#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySnapshot {
    pub id: String,
    pub label: String,
    pub frame: ScreenRect,
    pub work_area: ScreenRect,
    pub scale_factor: f64,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayOption {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeIdentity {
    pub key: String,
    pub label: Option<String>,
}

pub fn detect(app: &AppHandle) -> anyhow::Result<Vec<DisplaySnapshot>> {
    let primary = app
        .primary_monitor()
        .context("failed to query the primary monitor")?;
    let mut monitors = app
        .available_monitors()
        .context("failed to query connected monitors")?;
    monitors.sort_by_key(|monitor| {
        (
            monitor.position().y,
            monitor.position().x,
            monitor.name().cloned().unwrap_or_default(),
        )
    });
    let identities = super::native_monitor_identities(&monitors);
    let mut snapshots = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.into_iter().enumerate() {
        let identity = identities
            .get(index)
            .cloned()
            .unwrap_or_else(|| fallback_identity(&monitor));
        let id = format!(
            "{}:{}",
            super::monitor_id_prefix(),
            hex_digest(identity.key.as_bytes())
        );
        let label = identity
            .label
            .filter(|value| !value.trim().is_empty())
            .or_else(|| monitor.name().cloned())
            .unwrap_or_else(|| format!("{}×{}", monitor.size().width, monitor.size().height));
        let work_area = monitor.work_area();
        snapshots.push(DisplaySnapshot {
            id,
            label,
            frame: ScreenRect {
                x: monitor.position().x,
                y: monitor.position().y,
                w: monitor.size().width,
                h: monitor.size().height,
            },
            work_area: ScreenRect {
                x: work_area.position.x,
                y: work_area.position.y,
                w: work_area.size.width,
                h: work_area.size.height,
            },
            scale_factor: monitor.scale_factor(),
            primary: primary
                .as_ref()
                .is_some_and(|candidate| same_monitor(candidate, &monitor)),
        });
    }
    Ok(snapshots)
}

pub fn options(displays: &[DisplaySnapshot]) -> Vec<DisplayOption> {
    displays
        .iter()
        .map(|display| DisplayOption {
            id: display.id.clone(),
            label: display.label.clone(),
            width: display.frame.w,
            height: display.frame.h,
            scale_factor: display.scale_factor,
            primary: display.primary,
        })
        .collect()
}

fn fallback_identity(monitor: &Monitor) -> NativeIdentity {
    NativeIdentity {
        key: format!(
            "{}|{}x{}|{}|{},{}",
            monitor.name().map_or("", String::as_str),
            monitor.size().width,
            monitor.size().height,
            monitor.scale_factor(),
            monitor.position().x,
            monitor.position().y
        ),
        label: monitor.name().cloned().or_else(|| {
            Some(format!(
                "{}×{}",
                monitor.size().width,
                monitor.size().height
            ))
        }),
    }
}

fn same_monitor(left: &Monitor, right: &Monitor) -> bool {
    left.position() == right.position()
        && left.size() == right.size()
        && left.name() == right.name()
}

fn hex_digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
