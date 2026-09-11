//! Stable-enough GTK monitor identity shared by X11 and Wayland.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gtk::prelude::{MonitorExt, ObjectExt};
use tauri::Monitor;
use tauri::{AppHandle, Manager};

use crate::platform::display::DisplaySnapshot;
use crate::platform::display::NativeIdentity;

static REFRESH_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn install_watch(app: &AppHandle) -> anyhow::Result<()> {
    let display = gtk::gdk::Display::default()
        .ok_or_else(|| anyhow::anyhow!("GTK display is unavailable for monitor discovery"))?;
    for index in 0..display.n_monitors() {
        if let Some(monitor) = display.monitor(index) {
            watch_properties(&monitor, app);
        }
    }
    let added_app = app.clone();
    display.connect_monitor_added(move |_, monitor| {
        watch_properties(monitor, &added_app);
        schedule_refresh(&added_app);
    });
    let removed_app = app.clone();
    display.connect_monitor_removed(move |_, _| schedule_refresh(&removed_app));
    if let Some(screen) = gtk::gdk::Screen::default() {
        let changed_app = app.clone();
        screen.connect_monitors_changed(move |_| schedule_refresh(&changed_app));
    }
    Ok(())
}

fn watch_properties(monitor: &gtk::gdk::Monitor, app: &AppHandle) {
    for property in ["geometry", "scale-factor"] {
        let app = app.clone();
        monitor.connect_notify_local(Some(property), move |_, _| schedule_refresh(&app));
    }
}

fn schedule_refresh(app: &AppHandle) {
    let generation = REFRESH_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(150), move || {
        if REFRESH_GENERATION.load(Ordering::Relaxed) != generation {
            return;
        }
        let detected = match crate::platform::display::detect(&app) {
            Ok(displays) => displays,
            Err(error) => {
                tracing::error!(%error, "failed to refresh Linux monitor inventory");
                return;
            }
        };
        tauri::async_runtime::spawn(async move {
            if let Err(error) = app
                .state::<crate::surfaces::SurfaceManager>()
                .reconcile_monitors(&app, Some(detected), "topology")
                .await
            {
                tracing::error!(%error, "failed to apply Linux monitor topology change");
            }
        });
    });
}

pub fn identities(monitors: &[Monitor]) -> Vec<NativeIdentity> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    monitors
        .iter()
        .map(|monitor| {
            let native = (0..display.n_monitors())
                .filter_map(|index| display.monitor(index))
                .find(|candidate| matches_monitor(candidate, monitor));
            let Some(native) = native else {
                return fallback(monitor);
            };
            let manufacturer = native.manufacturer().unwrap_or_default();
            let model = native.model().unwrap_or_default();
            NativeIdentity {
                key: format!(
                    "{}|{}|{}x{}|{}",
                    manufacturer,
                    model,
                    native.width_mm(),
                    native.height_mm(),
                    monitor.name().map_or_else(
                        || format!("{},{}", monitor.position().x, monitor.position().y),
                        String::clone,
                    )
                ),
                label: match (manufacturer.is_empty(), model.is_empty()) {
                    (false, false) => Some(format!("{manufacturer} {model}")),
                    (false, true) => Some(manufacturer.to_string()),
                    (true, false) => Some(model.to_string()),
                    (true, true) => monitor.name().cloned(),
                },
            }
        })
        .collect()
}

fn matches_monitor(native: &gtk::gdk::Monitor, monitor: &Monitor) -> bool {
    let geometry = native.geometry();
    let scale = native.scale_factor();
    geometry.x().saturating_mul(scale) == monitor.position().x
        && geometry.y().saturating_mul(scale) == monitor.position().y
        && (geometry.width().saturating_mul(scale) as u32) == monitor.size().width
        && (geometry.height().saturating_mul(scale) as u32) == monitor.size().height
}

pub fn find(snapshot: &DisplaySnapshot) -> Option<gtk::gdk::Monitor> {
    let display = gtk::gdk::Display::default()?;
    (0..display.n_monitors())
        .filter_map(|index| display.monitor(index))
        .find(|monitor| {
            let geometry = monitor.geometry();
            let scale = monitor.scale_factor();
            geometry.x().saturating_mul(scale) == snapshot.frame.x
                && geometry.y().saturating_mul(scale) == snapshot.frame.y
                && (geometry.width().saturating_mul(scale) as u32) == snapshot.frame.w
                && (geometry.height().saturating_mul(scale) as u32) == snapshot.frame.h
        })
}

fn fallback(monitor: &Monitor) -> NativeIdentity {
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
        label: monitor.name().cloned(),
    }
}
