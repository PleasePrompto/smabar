//! Keeps the bar host at the origin the surface manager chose.
//!
//! Only the hidden proxy is the registered AppBar, so to Explorer the host is
//! an ordinary top-level window — and Explorer fits ordinary windows into a
//! work area it just shrank, with one `SetWindowPos` from its own thread a
//! few dozen milliseconds after `ABM_SETPOS` returned. That move reaches the
//! host as a `WM_WINDOWPOSCHANGED` whose position is not the one smabar asked
//! for, which is the only reliable moment to undo it.

use std::sync::Mutex;

use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    UI::WindowsAndMessaging::{
        GetWindowRect, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetWindowPos, WINDOWPOS, WM_APP, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING,
    },
};

/// Posted to the host after a foreign move so the correction runs once the
/// mover's own `SetWindowPos` has returned instead of nesting inside it.
const RECONCILE_MESSAGE: u32 = WM_APP + 0x5342; // "SB"

static INTENDED_ORIGIN: Mutex<Option<(i32, i32)>> = Mutex::new(None);

fn intended() -> Option<(i32, i32)> {
    *INTENDED_ORIGIN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Every deliberate move of the bar host records its target here first, so a
/// later `WM_WINDOWPOSCHANGED` can tell smabar's own moves from the shell's.
pub(super) fn record(x: i32, y: i32) {
    *INTENDED_ORIGIN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((x, y));
}

/// Host window messages that concern placement: the position report itself
/// and the deferred correction it may schedule.
pub(super) fn window_message(hwnd: HWND, message: u32, lparam: LPARAM) {
    if message == WM_WINDOWPOSCHANGING && lparam.0 != 0 {
        // SAFETY: Windows supplies a writable WINDOWPOS for this message only.
        let pos = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
        if let Err(error) = super::taskbar_order::adjust(hwnd, pos) {
            tracing::error!(%error, "failed to keep the bar behind the system taskbar; retrying on the next placement");
        }
        return;
    }
    if message == RECONCILE_MESSAGE {
        reconcile(hwnd);
        return;
    }
    if message != WM_WINDOWPOSCHANGED {
        return;
    }
    if let Err(error) = super::hit_test::refresh_cursor() {
        tracing::error!(
            %error,
            "failed to refresh Windows input routing after moving the bar window"
        );
    }
    let Some(intended) = intended() else {
        return;
    };
    if lparam.0 == 0 {
        return;
    }
    // SAFETY: for WM_WINDOWPOSCHANGED, lParam points to the WINDOWPOS of this
    // very message and stays valid for the duration of the handler.
    let pos = unsafe { &*(lparam.0 as *const WINDOWPOS) };
    if pos.flags.contains(SWP_NOMOVE) || (pos.x, pos.y) == intended {
        return;
    }
    // SAFETY: posts to the live host window; no pointers travel with it.
    if let Err(error) = unsafe { PostMessageW(Some(hwnd), RECONCILE_MESSAGE, WPARAM(0), LPARAM(0)) }
    {
        tracing::error!(
            %error,
            "failed to schedule the bar window move-back; the bar stays where the shell put it until its next resize"
        );
    }
}

/// Moves the host back to the recorded origin when something else moved it.
pub(super) fn reconcile(hwnd: HWND) {
    let Some((x, y)) = intended() else {
        return;
    };
    let mut rect = RECT::default();
    // SAFETY: rect is writable and hwnd is the live host window.
    if let Err(error) = unsafe { GetWindowRect(hwnd, &mut rect) } {
        tracing::error!(
            %error,
            "failed to read the bar window position after the Windows work area changed"
        );
        return;
    }
    if (rect.left, rect.top) == (x, y) {
        return;
    }
    // SAFETY: hwnd is the live host window; only its position changes.
    let moved = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    match moved {
        Ok(()) => tracing::info!(
            from_x = rect.left,
            from_y = rect.top,
            to_x = x,
            to_y = y,
            "bar window moved back after the Windows work area changed"
        ),
        Err(error) => tracing::error!(
            %error,
            to_x = x,
            to_y = y,
            "failed to move the bar window back after the Windows work area changed; resize the bar or restart it to reposition"
        ),
    }
}
