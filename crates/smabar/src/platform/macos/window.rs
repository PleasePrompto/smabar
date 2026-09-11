//! Native hover and transient presentation without activating the application.

use anyhow::Context;
use objc2::rc::{PartialInit, Retained};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow};
use objc2_foundation::NSRect;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::surfaces::SurfaceRole;

struct HoverState {
    app: AppHandle,
    role: SurfaceRole,
}

define_class!(
    // SAFETY: NSTrackingArea has no subclassing requirements. AppKit retains
    // this area through its view; its non-retained event owner is itself.
    #[unsafe(super = NSTrackingArea)]
    #[thread_kind = MainThreadOnly]
    #[ivars = HoverState]
    struct HoverTracking;

    impl HoverTracking {
        #[unsafe(method(mouseEntered:))]
        fn entered(&self, event: &NSEvent) {
            self.report(event, true);
        }

        #[unsafe(method(mouseMoved:))]
        fn moved(&self, event: &NSEvent) {
            self.report(event, true);
        }

        #[unsafe(method(mouseExited:))]
        fn exited(&self, event: &NSEvent) {
            self.report(event, false);
        }
    }
);

impl HoverTracking {
    fn report(&self, event: &NSEvent, inside: bool) {
        let state = self.ivars();
        let result = match state.role {
            SurfaceRole::Bar => {
                // Tauri's webview fills the content view. AppKit reports
                // logical points from the bottom; the shell uses top-left CSS pixels.
                let sample =
                    inside
                        .then(|| event.window(self.mtm()))
                        .flatten()
                        .and_then(|window| {
                            let view = window.contentView()?;
                            let point = event.locationInWindow();
                            Some([point.x, view.bounds().size.height - point.y])
                        });
                state
                    .app
                    .emit_to(SurfaceRole::Bar.label(), "bar-pointer-sample", sample)
            }
            SurfaceRole::Overlay => {
                state
                    .app
                    .emit_to(SurfaceRole::Bar.label(), "overlay-pointer", inside)
            }
            _ => return,
        };
        if let Err(error) = result {
            tracing::warn!(%error, surface = state.role.label(), "cannot report macOS hover; restart smabar");
        }
    }
}

pub fn install_hover(window: &WebviewWindow, role: SurfaceRole) -> anyhow::Result<()> {
    let app = window.app_handle().clone();
    window
        .with_webview(move |platform| {
            // SAFETY: Tauri supplies its live WKWebView (an NSView subclass) on the GUI thread.
            let view = unsafe { &*platform.inner().cast::<NSView>() };
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect;
            // WebKit filters DOM motion in inactive windows. Feed the existing
            // shell pointer channel directly; keep WebKit's own tracking intact.
            // SAFETY: the initializer has NSTrackingArea's documented signature;
            // AppKit does not dispatch events until addTrackingArea below.
            let area: Retained<HoverTracking> = unsafe {
                let area = HoverTracking::alloc(view.mtm()).set_ivars(HoverState { app, role });
                let owner = PartialInit::as_ptr(&area);
                msg_send![super(area), initWithRect: NSRect::ZERO, options: options,
                owner: owner, userInfo: std::ptr::null::<AnyObject>()]
            };
            view.addTrackingArea(&area);
        })
        .context("cannot install macOS hover tracking; restart smabar")
}

pub fn present_transient(window: &WebviewWindow) -> anyhow::Result<()> {
    window
        .with_webview(|platform| {
            // SAFETY: Tauri supplies the live NSWindow on its GUI thread.
            let native = unsafe { &*platform.ns_window().cast::<NSWindow>() };
            // Tauri's show() makes a macOS window key, which sends a leave to
            // the bar and closes a hover preview. Pinned flyouts and menus
            // request focus explicitly through the existing presentation path.
            native.orderFrontRegardless();
        })
        .context("cannot present macOS transient without taking focus")
}
