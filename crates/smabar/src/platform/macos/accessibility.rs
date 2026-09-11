//! Typed Accessibility reads and per-application window notifications.

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2_application_services::{AXError, AXObserver, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFRetained, CFRunLoop, CFRunLoopSource, CFString, CFType,
    CGPoint, CGSize, kCFRunLoopDefaultMode,
};

use crate::platform::macos_geometry::Frame;

pub(super) fn trusted(request: bool) -> bool {
    // SAFETY: no borrowed buffers; the prompt option has Apple's documented Boolean type.
    unsafe {
        if request {
            let options = CFDictionary::from_slices(
                &[objc2_application_services::kAXTrustedCheckOptionPrompt],
                &[CFBoolean::new(true)],
            );
            objc2_application_services::AXIsProcessTrustedWithOptions(Some(options.as_opaque()))
        } else {
            objc2_application_services::AXIsProcessTrusted()
        }
    }
}

fn attribute(element: &AXUIElement, name: &str) -> Result<CFRetained<CFType>, AXError> {
    let mut value = std::ptr::null();
    // SAFETY: value is an out pointer for the duration of the synchronous call.
    let error = unsafe {
        element.copy_attribute_value(&CFString::from_str(name), NonNull::from(&mut value))
    };
    check(error)?;
    let pointer = NonNull::new(value.cast_mut()).ok_or(AXError::NoValue)?;
    // SAFETY: CopyAttributeValue returned an owned +1 CoreFoundation reference.
    Ok(unsafe { CFRetained::from_raw(pointer) })
}

fn check(error: AXError) -> Result<(), AXError> {
    if error == AXError::Success {
        Ok(())
    } else {
        Err(error)
    }
}

pub(super) fn flag(window: &AXUIElement, name: &str) -> Result<bool, AXError> {
    match attribute(window, name) {
        Ok(value) => value
            .downcast::<CFBoolean>()
            .map(|value| value.value())
            .map_err(|_| AXError::Failure),
        Err(AXError::AttributeUnsupported | AXError::NoValue) => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn ordinary(window: &AXUIElement) -> Result<bool, AXError> {
    let role = attribute(window, "AXSubrole")?
        .downcast::<CFString>()
        .map_err(|_| AXError::Failure)?;
    Ok(role.to_string() == "AXStandardWindow"
        && !flag(window, "AXMinimized")?
        && !flag(window, "AXFullScreen")?)
}

pub(super) fn frame(window: &AXUIElement) -> Result<Frame, AXError> {
    let position = attribute(window, "AXPosition")?
        .downcast::<AXValue>()
        .map_err(|_| AXError::Failure)?;
    let size = attribute(window, "AXSize")?
        .downcast::<AXValue>()
        .map_err(|_| AXError::Failure)?;
    let mut point = CGPoint::default();
    let mut dimensions = CGSize::default();
    // SAFETY: the requested AX types match the concrete out-buffer types and sizes.
    let valid = unsafe {
        position.value(AXValueType::CGPoint, NonNull::from(&mut point).cast())
            && size.value(AXValueType::CGSize, NonNull::from(&mut dimensions).cast())
    };
    if !valid {
        return Err(AXError::Failure);
    }
    Ok(Frame {
        x: point.x,
        y: point.y,
        w: dimensions.width,
        h: dimensions.height,
    })
}

pub(super) fn set_frame(window: &AXUIElement, before: Frame, after: Frame) -> Result<(), AXError> {
    let mut position = CGPoint::new(after.x, after.y);
    let mut size = CGSize::new(after.w, after.h);
    // SAFETY: AXValue copies the correctly typed structs while they are alive.
    let position =
        unsafe { AXValue::new(AXValueType::CGPoint, NonNull::from(&mut position).cast()) }
            .ok_or(AXError::Failure)?;
    // SAFETY: size is a valid CGSize matching the supplied type.
    let size = unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(&mut size).cast()) }
        .ok_or(AXError::Failure)?;
    let mut changes = [
        (before.w != after.w || before.h != after.h, "AXSize", &size),
        (
            before.x != after.x || before.y != after.y,
            "AXPosition",
            &position,
        ),
    ];
    // Shrink before moving; when restoring a taller window, move first so
    // AppKit does not clamp the expanded height against its old position.
    if after.h > before.h {
        changes.swap(0, 1);
    }
    for (changed, name, value) in changes {
        if changed {
            // SAFETY: each AX attribute is paired with its documented AXValue type above.
            check(unsafe { window.set_attribute_value(&CFString::from_str(name), value) })?;
        }
    }
    Ok(())
}

pub(super) struct Watch {
    application: CFRetained<AXUIElement>,
    observer: CFRetained<AXObserver>,
    source: CFRetained<CFRunLoopSource>,
    run_loop: CFRetained<CFRunLoop>,
    signal: Arc<AtomicBool>,
    windows: Vec<CFRetained<AXUIElement>>,
    errors: HashSet<(&'static str, i32)>,
    pid: i32,
}

impl Watch {
    pub fn known_windows(&self) -> &[CFRetained<AXUIElement>] {
        &self.windows
    }

    pub fn new(pid: i32, signal: Arc<AtomicBool>) -> Result<Self, AXError> {
        // SAFETY: pid comes from NSWorkspace; a vanished process is handled by AX errors.
        let application = unsafe { AXUIElement::new_application(pid) };
        // SAFETY: live AX object; bound cross-process IPC so an unresponsive app cannot stall us.
        check(unsafe { application.set_messaging_timeout(0.1) })?;
        let mut pointer = std::ptr::null_mut();
        // SAFETY: callback only sets the retained signal; pointer is a valid out buffer.
        check(unsafe { AXObserver::create(pid, Some(changed), NonNull::from(&mut pointer)) })?;
        // SAFETY: successful AXObserverCreate returned one owned reference.
        let observer =
            unsafe { CFRetained::from_raw(NonNull::new(pointer).ok_or(AXError::Failure)?) };
        // SAFETY: observer is alive; the retained source is detached before observer destruction.
        let source = unsafe { observer.run_loop_source() };
        let run_loop = CFRunLoop::current().ok_or(AXError::Failure)?;
        // SAFETY: kCFRunLoopDefaultMode is a system-owned constant.
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
        let mut watch = Self {
            application,
            observer,
            source,
            run_loop,
            signal,
            windows: Vec::new(),
            errors: HashSet::new(),
            pid,
        };
        for name in [
            "AXWindowCreated",
            "AXFocusedWindowChanged",
            "AXApplicationActivated",
        ] {
            let application = watch.application.clone();
            watch.subscribe(&application, name);
        }
        Ok(watch)
    }

    pub fn windows(&mut self) -> Result<Vec<CFRetained<AXUIElement>>, AXError> {
        let value = attribute(&self.application, "AXWindows")?;
        let array = value.downcast::<CFArray>().map_err(|_| AXError::Failure)?;
        // SAFETY: AXWindows is documented to contain CF objects. Each element's AX type is checked below.
        let array = unsafe { array.cast_unchecked::<CFType>() };
        let windows = array
            .iter()
            .map(|value| {
                value
                    .downcast::<AXUIElement>()
                    .map_err(|_| AXError::Failure)
            })
            .collect::<Result<Vec<_>, _>>()?;
        for window in &windows {
            if !self.windows.contains(window) {
                for name in ["AXMoved", "AXResized", "AXUIElementDestroyed"] {
                    self.subscribe(window, name);
                }
            }
        }
        self.windows = windows.clone();
        Ok(windows)
    }

    fn subscribe(&mut self, element: &AXUIElement, name: &str) {
        // SAFETY: signal's allocation outlives all subscriptions; callbacks run on this worker's run loop.
        let error = unsafe {
            self.observer.add_notification(
                element,
                &CFString::from_str(name),
                Arc::as_ptr(&self.signal).cast_mut().cast(),
            )
        };
        if !matches!(
            error,
            AXError::Success | AXError::NotificationAlreadyRegistered
        ) {
            self.report("observe window changes", error);
        }
    }

    pub fn report(&mut self, operation: &'static str, error: AXError) {
        if self.errors.insert((operation, error.0)) {
            tracing::warn!(
                pid = self.pid,
                operation,
                ?error,
                "macOS window reservation could not complete an Accessibility operation; check the app's Accessibility support"
            );
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: the system constant is valid; callbacks cannot run after the source is removed on its owner thread.
        self.run_loop
            .remove_source(Some(&self.source), unsafe { kCFRunLoopDefaultMode });
    }
}

unsafe extern "C-unwind" fn changed(
    _observer: NonNull<AXObserver>,
    _element: NonNull<AXUIElement>,
    _notification: NonNull<CFString>,
    context: *mut c_void,
) {
    // SAFETY: Watch::subscribe passes its Arc's allocation and removes the source before dropping that Arc.
    if let Some(signal) = unsafe { context.cast::<AtomicBool>().as_ref() } {
        signal.store(true, Ordering::Relaxed);
    }
}
