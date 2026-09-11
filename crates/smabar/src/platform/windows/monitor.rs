//! Persistent Windows display identity through the active CCD paths.

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, Monitor};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::ERROR_SUCCESS;

use crate::platform::display::NativeIdentity;

static REFRESH_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn schedule_refresh(app: AppHandle) {
    let generation = REFRESH_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if REFRESH_GENERATION.load(Ordering::Relaxed) != generation {
            return;
        }
        let detected = match crate::platform::display::detect(&app) {
            Ok(displays) => displays,
            Err(error) => {
                tracing::error!(%error, "failed to refresh Windows monitor inventory");
                return;
            }
        };
        if let Err(error) = app
            .state::<crate::surfaces::SurfaceManager>()
            .reconcile_monitors(&app, Some(detected), "topology")
            .await
        {
            tracing::error!(%error, "failed to apply Windows monitor topology change");
        }
    });
}

pub fn identities(monitors: &[Monitor]) -> Vec<NativeIdentity> {
    let paths = active_paths().unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to read stable Windows monitor identities; using active display names");
        HashMap::new()
    });
    monitors
        .iter()
        .map(|monitor| {
            let name = monitor.name().cloned().unwrap_or_default();
            paths
                .get(&name.to_ascii_uppercase())
                .cloned()
                .unwrap_or(NativeIdentity {
                    key: if name.is_empty() {
                        format!("{},{}", monitor.position().x, monitor.position().y)
                    } else {
                        name.clone()
                    },
                    label: (!name.is_empty()).then_some(name),
                })
        })
        .collect()
}

fn active_paths() -> anyhow::Result<HashMap<String, NativeIdentity>> {
    let mut path_count = 0;
    let mut mode_count = 0;
    let status = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if status != ERROR_SUCCESS {
        anyhow::bail!(
            "GetDisplayConfigBufferSizes failed with status {}",
            status.0
        );
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    let status = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if status != ERROR_SUCCESS {
        anyhow::bail!("QueryDisplayConfig failed with status {}", status.0);
    }
    paths.truncate(path_count as usize);
    let mut identities = HashMap::new();
    for path in paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        let source_status = unsafe { DisplayConfigGetDeviceInfo(&mut source.header) };
        if source_status != 0 {
            continue;
        }
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        target.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        target.header.adapterId = path.targetInfo.adapterId;
        target.header.id = path.targetInfo.id;
        let target_status = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) };
        if target_status != 0 {
            continue;
        }
        let source_name = utf16(&source.viewGdiDeviceName);
        let device_path = utf16(&target.monitorDevicePath);
        if source_name.is_empty() || device_path.is_empty() {
            continue;
        }
        let friendly = utf16(&target.monitorFriendlyDeviceName);
        identities.insert(
            source_name.to_ascii_uppercase(),
            NativeIdentity {
                key: device_path,
                label: (!friendly.is_empty()).then_some(friendly),
            },
        );
    }
    Ok(identities)
}

fn utf16(value: &[u16]) -> String {
    String::from_utf16_lossy(
        &value[..value
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(value.len())],
    )
}
