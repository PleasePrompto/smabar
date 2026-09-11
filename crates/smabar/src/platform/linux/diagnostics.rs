//! Native surface lifecycle, enabled through the shared SMABAR_LOG filter.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::{ObjectExt, WidgetExt};

use crate::surfaces::SurfaceRole;

pub(super) fn observe(window: &gtk::ApplicationWindow, role: SurfaceRole) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    report(window, role, "created");
    let painted = Rc::new(Cell::new(false));
    let mapped_painted = Rc::clone(&painted);
    window.connect_map(move |window| {
        mapped_painted.set(false);
        report(window, role, "mapped");
    });
    window.connect_unmap(move |window| report(window, role, "unmapped"));
    window.connect_realize(move |window| {
        report(window, role, "realized");
        if let Some(clock) = window.frame_clock() {
            let weak_window = window.downgrade();
            let painted = Rc::clone(&painted);
            clock.connect_after_paint(move |_| {
                if !painted.replace(true)
                    && let Some(window) = weak_window.upgrade()
                {
                    report(&window, role, "first-paint");
                }
            });
        }
    });
}

fn report(window: &gtk::ApplicationWindow, role: SurfaceRole, event: &str) {
    tracing::debug!(
        surface = role.label(),
        event,
        visible = window.is_visible(),
        mapped = window.is_mapped(),
        realized = window.is_realized(),
        width = window.allocated_width(),
        height = window.allocated_height(),
        opacity = window.opacity(),
        frame = window.frame_clock().map(|clock| clock.frame_counter()),
        "native surface lifecycle"
    );
}
