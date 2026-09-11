//! Observe window moves/resizes and restore only frames we still own.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationPolicy, NSEvent, NSWorkspace};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFRetained, CFRunLoop, CFRunLoopRunResult, kCFRunLoopDefaultMode};

use super::accessibility::{self as ax, Watch};
use super::strut::Change;
use crate::platform::macos_geometry::{Area, Frame};

const APP_SCAN_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_WAIT: Duration = Duration::from_millis(50);

struct Adjusted {
    window: CFRetained<AXUIElement>,
    before: Frame,
    after: Frame,
    requested: Frame,
    limited: bool,
}

pub(super) fn run(receive: mpsc::Receiver<Change>) {
    let dirty = Arc::new(AtomicBool::new(true));
    let mut area = None;
    let mut watches = HashMap::new();
    let mut refused = HashSet::new();
    let mut adjusted = Vec::new();
    let mut last_scan = None;
    let mut last_trusted = None;
    loop {
        let change = if area.is_none() {
            match receive.recv() {
                Ok(change) => Some(change),
                Err(_) => break,
            }
        } else {
            match receive.try_recv() {
                Ok(change) => Some(change),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        };
        match change {
            Some(Change::Stop) => break,
            Some(Change::Area(next)) if next != area => {
                restore(&mut adjusted);
                area = next;
                dirty.store(true, Ordering::Relaxed);
                last_scan = None;
                if area.is_none() {
                    watches.clear();
                    refused.clear();
                }
            }
            Some(Change::Area(_)) | None => {}
        }
        let Some(area) = area else {
            continue;
        };
        let granted = ax::trusted(false);
        if last_trusted != Some(granted) {
            tracing::info!(
                granted,
                "macOS screen reservation Accessibility permission changed"
            );
            last_trusted = Some(granted);
            refused.clear();
            last_scan = None;
            dirty.store(true, Ordering::Relaxed);
        }
        if !granted {
            watches.clear();
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        autoreleasepool(|_| {
            if last_scan.is_none_or(|time: Instant| time.elapsed() >= APP_SCAN_INTERVAL) {
                refresh_apps(&mut watches, &mut refused, &dirty);
                last_scan = Some(Instant::now());
                // AX notifications are best effort while apps launch, animate
                // or ignore a subscription. The existing app scan also retries
                // their windows, without waiting for another user action.
                dirty.store(true, Ordering::Relaxed);
            }
            // Let the user finish an interactive drag before correcting its result.
            if NSEvent::pressedMouseButtons() == 0 && dirty.swap(false, Ordering::Relaxed) {
                reconcile(area, &mut watches, &mut adjusted);
            }
            // SAFETY: the run-loop mode is a system-owned constant.
            let outcome = CFRunLoop::run_in_mode(
                unsafe { kCFRunLoopDefaultMode },
                EVENT_WAIT.as_secs_f64(),
                false,
            );
            if outcome == CFRunLoopRunResult::Finished {
                std::thread::sleep(EVENT_WAIT);
            }
        });
    }
    restore(&mut adjusted);
    // All observer sources are removed on the thread that dispatches them.
    drop(watches);
    tracing::debug!("macOS screen reservation released");
}

fn refresh_apps(
    watches: &mut HashMap<i32, Watch>,
    refused: &mut HashSet<i32>,
    dirty: &Arc<AtomicBool>,
) {
    let pids: HashSet<_> = NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .map(|app| app.processIdentifier())
        .filter(|pid| u32::try_from(*pid).ok() != Some(std::process::id()))
        .collect();
    watches.retain(|pid, _| pids.contains(pid));
    refused.retain(|pid| pids.contains(pid));
    for pid in pids {
        if watches.contains_key(&pid) || refused.contains(&pid) {
            continue;
        }
        match Watch::new(pid, Arc::clone(dirty)) {
            Ok(watch) => {
                watches.insert(pid, watch);
                dirty.store(true, Ordering::Relaxed);
            }
            Err(error) => {
                tracing::warn!(
                    pid,
                    ?error,
                    "cannot observe macOS application windows; reopen that app to retry screen reservation"
                );
                refused.insert(pid);
            }
        }
    }
}

fn reconcile(area: Area, watches: &mut HashMap<i32, Watch>, adjusted: &mut Vec<Adjusted>) {
    let mut live = Vec::new();
    for watch in watches.values_mut() {
        let windows = match watch.windows() {
            Ok(windows) => windows,
            Err(error) => {
                watch.report("list windows", error);
                // A busy app has not closed its windows. Keep their originals
                // until a successful list or process exit proves otherwise.
                live.extend_from_slice(watch.known_windows());
                continue;
            }
        };
        for window in windows {
            live.push(window.clone());
            let before = match ax::ordinary(&window).and_then(|ordinary| {
                if ordinary {
                    ax::frame(&window).map(Some)
                } else {
                    Ok(None)
                }
            }) {
                Ok(Some(frame)) => frame,
                Ok(None) => continue,
                Err(error) => {
                    watch.report("read window geometry", error);
                    continue;
                }
            };
            if let Some(previous) = adjusted.iter_mut().find(|saved| saved.window == window)
                && previous.requested.same(before)
            {
                // AppKit can finish the requested resize after AXSet returns.
                previous.after = before;
                previous.limited = false;
            }
            let Some(after) = area.constrain(before) else {
                continue;
            };
            // Some apps clamp to a minimum size or reject moves. Their own resize
            // notification must not create an endless correction loop.
            if adjusted
                .iter()
                .any(|saved| saved.window == window && saved.limited && saved.after.same(before))
            {
                continue;
            }
            if let Err(error) = ax::set_frame(&window, before, after) {
                watch.report("resize window around the bar", error);
            }
            let actual = match ax::frame(&window) {
                Ok(frame) => frame,
                Err(error) => {
                    watch.report("read adjusted window geometry", error);
                    continue;
                }
            };
            let limited = !actual.same(after);
            if let Some(previous) = adjusted.iter_mut().find(|saved| saved.window == window) {
                if !previous.after.same(before) {
                    previous.before = before;
                }
                previous.after = actual;
                previous.requested = after;
                previous.limited = limited;
            } else {
                adjusted.push(Adjusted {
                    window,
                    before,
                    after: actual,
                    requested: after,
                    limited,
                });
            }
            tracing::debug!(
                ?before,
                ?after,
                ?actual,
                limited,
                "applied macOS window reservation"
            );
        }
    }
    adjusted.retain(|saved| live.contains(&saved.window));
}

fn restore(adjusted: &mut Vec<Adjusted>) {
    for saved in adjusted.drain(..) {
        // Closed, minimized/fullscreen, or user-moved windows no longer belong to this adjustment.
        let current = match ax::ordinary(&saved.window).and_then(|ordinary| {
            if ordinary {
                ax::frame(&saved.window).map(Some)
            } else {
                Ok(None)
            }
        }) {
            Ok(Some(frame)) if frame.same(saved.after) || frame.same(saved.requested) => frame,
            Ok(_) => continue,
            Err(error) => {
                tracing::debug!(
                    ?error,
                    "macOS reservation no longer owns the closed or inaccessible window"
                );
                continue;
            }
        };
        if let Err(error) = ax::set_frame(&saved.window, current, saved.before) {
            tracing::warn!(
                ?error,
                "cannot restore a macOS window after releasing screen reservation; resize it manually"
            );
        }
    }
}
