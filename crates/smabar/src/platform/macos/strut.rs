//! Keep other applications out of the bar through the public Accessibility API.
//! macOS exposes no writable NSScreen work area to third-party docks.

use std::sync::{Mutex, mpsc};
use std::thread::JoinHandle;

use anyhow::Context;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWindow};
use smabar_core::platform::Rect;
use tauri::{PhysicalPosition, WebviewWindow};

use super::{accessibility, reservation};
use crate::platform::macos_geometry::{Area, Frame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEdge {
    Top,
    Bottom,
}

pub(super) enum Change {
    Area(Option<Area>),
    Stop,
}

struct Worker {
    send: mpsc::Sender<Change>,
    thread: JoinHandle<()>,
}

// One native reservation belongs to the single app process, just like the
// Windows AppBar registration. shutdown explicitly releases it before exit.
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

pub fn apply(
    window: &WebviewWindow,
    dock: Option<(DockEdge, Rect)>,
    _target_origin: Option<PhysicalPosition<i32>>,
) -> anyhow::Result<()> {
    let bar = window.clone();
    window.run_on_main_thread(move || {
        // Queue release as well as reserve so a pending geometry update cannot
        // re-enable reservation after the user has switched to another mode.
        match dock.map(|(edge, rect)| area(&bar, edge, rect)).transpose().and_then(send) {
            Ok(()) => {}
            Err(error) => tracing::error!(%error, "cannot reserve macOS screen space; reopen bar settings and check Accessibility permission"),
        }
    }).context("failed to schedule macOS screen reservation")
}

fn area(window: &WebviewWindow, edge: DockEdge, rect: Rect) -> anyhow::Result<Area> {
    let mtm =
        MainThreadMarker::new().context("screen reservation requires the macOS main thread")?;
    let pointer = window
        .ns_window()
        .context("cannot access the bar's native window")?;
    // SAFETY: Tauri owns this NSWindow for the live WebviewWindow; access is confined to its main thread.
    let native = unsafe { &*pointer.cast::<NSWindow>() };
    let screen = native
        .screen()
        .context("bar is not on a connected screen")?;
    let screens = NSScreen::screens(mtm);
    let primary = screens
        .firstObject()
        .context("no macOS display is available")?;
    let primary_height = primary.frame().size.height;
    let convert = |rect: objc2_foundation::NSRect| Frame {
        x: rect.origin.x,
        y: primary_height - rect.origin.y - rect.size.height,
        w: rect.size.width,
        h: rect.size.height,
    };
    let frame = convert(screen.frame());
    let mut usable = convert(screen.visibleFrame());
    let bar = convert(native.frame());
    match edge {
        DockEdge::Top => {
            let bottom = usable.y + usable.h;
            usable.y = usable.y.max(bar.y + f64::from(rect.y) + f64::from(rect.h));
            usable.h = bottom - usable.y;
        }
        DockEdge::Bottom => {
            usable.h = (bar.y + f64::from(rect.y)).min(usable.y + usable.h) - usable.y;
        }
    }
    anyhow::ensure!(
        frame.valid() && usable.valid(),
        "bar leaves no usable screen area; reduce its height"
    );
    Ok(Area {
        screen: frame,
        usable,
    })
}

fn send(area: Option<Area>) -> anyhow::Result<()> {
    let mut worker = WORKER
        .lock()
        .map_err(|_| anyhow::anyhow!("macOS reservation state was poisoned; restart smabar"))?;
    if worker.is_none() {
        if area.is_none() {
            return Ok(());
        }
        let (send, receive) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("smabar-reservation".into())
            .spawn(move || reservation::run(receive))
            .context("failed to start the macOS reservation worker")?;
        *worker = Some(Worker { send, thread });
    }
    if let Some(worker) = worker.as_ref() {
        worker
            .send
            .send(Change::Area(area))
            .context("macOS reservation worker stopped; restart smabar")?;
    }
    Ok(())
}

pub fn permission(request: bool) -> bool {
    accessibility::trusted(request)
}

pub fn shutdown() -> anyhow::Result<()> {
    let worker = WORKER
        .lock()
        .map_err(|_| anyhow::anyhow!("macOS reservation state was poisoned"))?
        .take();
    if let Some(worker) = worker {
        let sent = worker.send.send(Change::Stop);
        worker
            .thread
            .join()
            .map_err(|_| anyhow::anyhow!("macOS reservation worker panicked during shutdown"))?;
        sent.context("macOS reservation worker stopped before shutdown")?;
    }
    Ok(())
}
