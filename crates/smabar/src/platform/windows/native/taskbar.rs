//! Explorer taskbar restart recovery for the AppBar proxy.

use std::sync::{
    TryLockError,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW},
};
use windows::core::w;

use super::{APPBAR_CALLS, appbar, lock_state, raw_hwnd, reassert_appbar};

static ERROR_REPORTED: AtomicBool = AtomicBool::new(false);

pub(super) fn register_message() -> anyhow::Result<u32> {
    // SAFETY: the static UTF-16 string is valid for the duration of the call.
    let message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    if message == 0 {
        return Err(windows::core::Error::from_thread())
            .context("failed to register the Windows TaskbarCreated message");
    }
    ERROR_REPORTED.store(false, Ordering::Relaxed);
    Ok(message)
}

pub(super) fn recover(hwnd: HWND, message: u32) {
    let host = lock_state().and_then(|state| {
        state
            .hwnd
            .context("bar HWND is unavailable after Explorer restart")
    });
    if let Err(error) = host.and_then(|raw| super::super::taskbar_order::refresh(HWND(raw as _))) {
        report_once(error);
    }
    let calls = match APPBAR_CALLS.try_lock() {
        Ok(calls) => calls,
        Err(TryLockError::WouldBlock) => {
            // SHAppBarMessage can dispatch sent messages reentrantly. Queue the
            // recovery rather than deadlocking or losing the one restart signal.
            if let Err(error) = unsafe { PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0)) } {
                mark_unregistered(hwnd);
                report_once(error);
            }
            return;
        }
        Err(TryLockError::Poisoned(_)) => {
            report_once("Windows AppBar call lock is poisoned");
            return;
        }
    };

    let callback_message = {
        let mut state = match lock_state() {
            Ok(state) => state,
            Err(error) => {
                report_once(error);
                return;
            }
        };
        if state.appbar_hwnd != Some(raw_hwnd(hwnd))
            || state.reservation.is_none()
            || state.applied.is_none()
        {
            return;
        }
        state.appbar_registered = false;
        state.callback_message
    };

    // Explorer also broadcasts TaskbarCreated for some primary-display DPI
    // changes, where the old registration can still be live. Remove it first;
    // rejection is expected after a real Explorer restart.
    let remove_error = appbar::remove(hwnd).err();
    if let Err(mut error) = appbar::register(hwnd, callback_message) {
        if let Some(remove_error) = remove_error {
            error = error.context(format!(
                "the previous AppBar registration also could not be removed: {remove_error:#}"
            ));
        }
        report_once(error);
        return;
    }
    if let Ok(mut state) = lock_state()
        && state.appbar_hwnd == Some(raw_hwnd(hwnd))
    {
        state.appbar_registered = true;
    } else {
        report_once("Windows AppBar state changed while restoring the reservation");
        return;
    }

    ERROR_REPORTED.store(false, Ordering::Relaxed);
    drop(calls);
    reassert_appbar(
        hwnd,
        "Windows AppBar reservation restored after Explorer restart",
    );
}

fn mark_unregistered(hwnd: HWND) {
    if let Ok(mut state) = lock_state()
        && state.appbar_hwnd == Some(raw_hwnd(hwnd))
    {
        state.appbar_registered = false;
    }
}

fn report_once(error: impl std::fmt::Display) {
    if !ERROR_REPORTED.swap(true, Ordering::Relaxed) {
        tracing::error!(
            %error,
            "failed to restore the Windows AppBar after Explorer restarted; smabar will retry on the next bar geometry update, or restart smabar if maximized windows overlap it"
        );
    }
}
