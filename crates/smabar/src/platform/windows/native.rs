//! Thin Win32 boundary shared by input hit-testing and AppBar reservation.

use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};

use anyhow::{Context, anyhow, bail};
use smabar_core::platform::Rect;
use tauri::{AppHandle, Manager, WebviewWindow};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{
            ABN_FULLSCREENAPP, ABN_POSCHANGED, DefSubclassProc, RemoveWindowSubclass,
            SetWindowSubclass,
        },
        WindowsAndMessaging::{
            RegisterWindowMessageW, WM_ACTIVATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED,
            WM_NCDESTROY, WM_WINDOWPOSCHANGED,
        },
    },
};
use windows::core::w;

use super::appbar::{self, AppliedReservation, Reservation};
use crate::platform::windows_geometry::{DockEdge, scale_for_dpi};

mod taskbar;

const SUBCLASS_ID: usize = 0x534D_4142; // "SMAB"

static STATE: OnceLock<Mutex<NativeState>> = OnceLock::new();
// Serializes shell calls. AppBar notifications use try_lock because the shell
// may send one synchronously from inside SHAppBarMessage.
static APPBAR_CALLS: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct NativeState {
    app: Option<AppHandle>,
    hwnd: Option<isize>,
    appbar_hwnd: Option<isize>,
    callback_message: u32,
    taskbar_created_message: u32,
    reservation: Option<Reservation>,
    applied: Option<AppliedReservation>,
    appbar_registered: bool,
}

fn state() -> &'static Mutex<NativeState> {
    STATE.get_or_init(|| Mutex::new(NativeState::default()))
}

fn lock_state() -> anyhow::Result<MutexGuard<'static, NativeState>> {
    state()
        .lock()
        .map_err(|_| anyhow!("Windows native window state is poisoned"))
}

pub(super) fn raw_hwnd(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn ensure_window(state: &NativeState, hwnd: HWND) -> anyhow::Result<()> {
    if state.hwnd == Some(raw_hwnd(hwnd)) {
        Ok(())
    } else {
        bail!("Windows native window integration is not installed for this window")
    }
}

pub fn window_hwnd(window: &WebviewWindow) -> anyhow::Result<HWND> {
    let hwnd = window
        .hwnd()
        .context("failed to resolve the Windows bar HWND")?;
    // Tauri 2.11 uses windows 0.61 while the app pins 0.62. Both wrappers
    // carry the same raw Win32 pointer, so cross the version boundary here.
    Ok(HWND(hwnd.0 as _))
}

pub fn install(window: &WebviewWindow) -> anyhow::Result<()> {
    let hwnd = window_hwnd(window)?;
    // SAFETY: the static UTF-16 string is valid for the duration of the call.
    let callback_message = unsafe { RegisterWindowMessageW(w!("smabar.AppBarCallback")) };
    if callback_message == 0 {
        return Err(windows::core::Error::from_thread())
            .context("failed to register the Windows AppBar callback message");
    }
    let taskbar_created_message = taskbar::register_message()?;

    {
        let mut state = lock_state()?;
        if state.hwnd.is_some() && state.hwnd != Some(raw_hwnd(hwnd)) {
            bail!("Windows native window integration is already attached to another window");
        }
        state.app = Some(window.app_handle().clone());
        state.hwnd = Some(raw_hwnd(hwnd));
        state.appbar_hwnd = None;
        state.callback_message = callback_message;
        state.taskbar_created_message = taskbar_created_message;
        state.reservation = None;
        state.applied = None;
        state.appbar_registered = false;
    }

    // SAFETY: setup runs on the HWND's creating thread; the callback and ID
    // are static, and all callback state outlives the window.
    let installed =
        unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool() };
    if !installed {
        *lock_state()? = NativeState::default();
        return Err(windows::core::Error::from_thread())
            .context("failed to install the Windows bar window subclass");
    }

    let appbar_hwnd = match super::proxy::create() {
        Ok(appbar_hwnd) => appbar_hwnd,
        Err(error) => {
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
            *lock_state()? = NativeState::default();
            return Err(error);
        }
    };
    lock_state()?.appbar_hwnd = Some(raw_hwnd(appbar_hwnd));

    if let Err(error) = super::hit_test::install(hwnd) {
        let _ = super::proxy::destroy(appbar_hwnd);
        // SAFETY: the callback and ID were installed immediately above.
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        *lock_state()? = NativeState::default();
        return Err(error);
    }

    tracing::info!(
        host_hwnd = raw_hwnd(hwnd),
        appbar_hwnd = raw_hwnd(appbar_hwnd),
        callback_message,
        taskbar_created_message,
        "Windows native input shaping and AppBar integration initialized"
    );
    Ok(())
}

pub fn set_reservation(hwnd: HWND, dock: Option<(DockEdge, Rect)>) -> anyhow::Result<()> {
    let _calls = APPBAR_CALLS
        .lock()
        .map_err(|_| anyhow!("Windows AppBar call lock is poisoned"))?;
    match dock {
        Some((edge, bar)) => apply_requested_reservation(hwnd, Reservation { edge, bar }),
        None => clear_requested_reservation(hwnd),
    }
}

pub fn shutdown() -> anyhow::Result<()> {
    let input_result = super::hit_test::uninstall();
    let _calls = APPBAR_CALLS
        .lock()
        .map_err(|_| anyhow!("Windows AppBar call lock is poisoned"))?;
    let hwnd = {
        let state = lock_state()?;
        state.hwnd.map(|raw| HWND(raw as _))
    };
    let appbar_result = match hwnd {
        Some(hwnd) => clear_requested_reservation(hwnd),
        None => Ok(()),
    };
    let proxy_result = destroy_proxy();
    input_result.and(appbar_result).and(proxy_result)
}

fn destroy_proxy() -> anyhow::Result<()> {
    let proxy = {
        let mut state = lock_state()?;
        state.appbar_registered = false;
        state.appbar_hwnd.take().map(|raw| HWND(raw as _))
    };
    match proxy {
        Some(proxy) => super::proxy::destroy(proxy),
        None => Ok(()),
    }
}

fn apply_requested_reservation(hwnd: HWND, reservation: Reservation) -> anyhow::Result<()> {
    let (appbar_hwnd, callback_message, already_registered) = {
        let state = lock_state()?;
        ensure_window(&state, hwnd)?;
        (
            state
                .appbar_hwnd
                .map(|raw| HWND(raw as _))
                .context("Windows AppBar proxy is not initialized")?,
            state.callback_message,
            state.appbar_registered,
        )
    };

    let Some(thickness) = appbar::thickness(hwnd, reservation)? else {
        tracing::debug!(bar = ?reservation.bar, "Windows AppBar reservation deferred until the shell measures a bar inside its window");
        return Ok(());
    };
    let newly_registered = if already_registered {
        false
    } else {
        appbar::register(appbar_hwnd, callback_message)?;
        let mut state = lock_state()?;
        ensure_window(&state, hwnd)?;
        state.appbar_registered = true;
        true
    };

    match appbar::position(appbar_hwnd, hwnd, reservation, thickness) {
        Ok(applied) => {
            let mut state = lock_state()?;
            ensure_window(&state, hwnd)?;
            state.reservation = Some(reservation);
            state.applied = Some(applied);
            drop(state);
            appbar::log(
                hwnd,
                appbar_hwnd,
                reservation.bar,
                applied,
                "Windows AppBar reservation applied",
            );
            Ok(())
        }
        Err(mut error) => {
            if newly_registered {
                match appbar::remove(appbar_hwnd) {
                    Ok(()) => {
                        if let Ok(mut state) = lock_state()
                            && state.hwnd == Some(raw_hwnd(hwnd))
                        {
                            state.appbar_registered = false;
                        }
                    }
                    Err(cleanup) => {
                        error = error.context(format!(
                            "the failed AppBar registration also could not be released: {cleanup:#}"
                        ));
                    }
                }
            }
            Err(error)
        }
    }
}

fn clear_requested_reservation(hwnd: HWND) -> anyhow::Result<()> {
    let (appbar_hwnd, registered) = {
        let state = lock_state()?;
        ensure_window(&state, hwnd)?;
        (
            state.appbar_hwnd.map(|raw| HWND(raw as _)),
            state.appbar_registered,
        )
    };
    if registered {
        let appbar_hwnd = appbar_hwnd.context("Windows AppBar proxy is not initialized")?;
        appbar::remove(appbar_hwnd)?;
    }

    let mut state = lock_state()?;
    if state.hwnd == Some(raw_hwnd(hwnd)) {
        state.appbar_registered = false;
        state.reservation = None;
        state.applied = None;
    }
    drop(state);
    if registered {
        tracing::info!(
            host_hwnd = raw_hwnd(hwnd),
            appbar_hwnd = appbar_hwnd.map(raw_hwnd),
            "Windows AppBar reservation released"
        );
    }
    Ok(())
}

fn reassert_appbar(appbar_hwnd: HWND, reason: &'static str) {
    let _calls = match APPBAR_CALLS.try_lock() {
        Ok(calls) => calls,
        Err(TryLockError::WouldBlock) => return,
        Err(TryLockError::Poisoned(_)) => {
            tracing::error!("Windows AppBar call lock is poisoned; reservation was not reapplied");
            return;
        }
    };
    let appbar_state = match lock_state() {
        Ok(state)
            if state.appbar_hwnd == Some(raw_hwnd(appbar_hwnd))
                && state.appbar_registered
                && state.reservation.is_some()
                && state.applied.is_some() =>
        {
            state
                .hwnd
                .map(|raw| HWND(raw as _))
                .zip(state.reservation)
                .zip(state.applied)
                .map(|((host, reservation), applied)| (host, reservation, applied))
        }
        Ok(_) => None,
        Err(error) => {
            tracing::error!(%error, "failed to read Windows AppBar state");
            None
        }
    };
    let Some((host, reservation, previous)) = appbar_state else {
        return;
    };

    let applied = appbar::reposition(
        appbar_hwnd,
        host,
        reservation,
        appbar::logical_thickness(previous),
    );
    match applied {
        Ok(applied) => {
            if let Ok(mut state) = lock_state()
                && state.appbar_hwnd == Some(raw_hwnd(appbar_hwnd))
            {
                state.applied = Some(applied);
            }
            appbar::log(host, appbar_hwnd, reservation.bar, applied, reason);
        }
        Err(error) => {
            tracing::error!(%error, "failed to reapply Windows AppBar reservation");
        }
    }
}

fn notify_activation(host: HWND) {
    let _calls = match APPBAR_CALLS.try_lock() {
        Ok(calls) => calls,
        Err(TryLockError::WouldBlock) => return,
        Err(TryLockError::Poisoned(_)) => {
            tracing::error!("Windows AppBar call lock is poisoned; shell notification was skipped");
            return;
        }
    };
    let appbar_hwnd = state().lock().ok().and_then(|state| {
        (state.hwnd == Some(raw_hwnd(host)) && state.appbar_registered)
            .then(|| state.appbar_hwnd.map(|raw| HWND(raw as _)))
            .flatten()
    });
    if let Some(appbar_hwnd) = appbar_hwnd {
        appbar::activate(appbar_hwnd);
    }
}

pub(super) fn window_scale(hwnd: HWND) -> anyhow::Result<(u32, f64)> {
    // SAFETY: hwnd is the live bar window; the function has no pointer output.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let scale = scale_for_dpi(dpi).context("Windows returned zero DPI for the bar window")?;
    Ok((dpi, scale))
}

fn appbar_for_host(hwnd: HWND) -> Option<HWND> {
    let state = state().lock().ok()?;
    (state.hwnd == Some(raw_hwnd(hwnd)))
        .then(|| state.appbar_hwnd.map(|raw| HWND(raw as _)))
        .flatten()
}

pub(super) fn appbar_notification(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
    let notification = wparam.0 as u32;
    if notification == ABN_POSCHANGED {
        reassert_appbar(
            hwnd,
            "Windows AppBar reservation reapplied after ABN_POSCHANGED",
        );
        return;
    }
    if notification != ABN_FULLSCREENAPP {
        return;
    }
    let app = match lock_state() {
        Ok(state) => (state.appbar_hwnd == Some(raw_hwnd(hwnd)) && state.appbar_registered)
            .then(|| state.app.clone())
            .flatten(),
        Err(error) => {
            tracing::error!(%error, "failed to read Windows AppBar fullscreen state");
            None
        }
    };
    if let Some(app) = app {
        super::window::apply_fullscreen_signal(&app, lparam.0 != 0);
    }
}

pub(super) fn proxy_window_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> bool {
    let proxy_state = state().lock().ok().and_then(|state| {
        (state.appbar_hwnd == Some(raw_hwnd(hwnd))).then_some((
            state.callback_message,
            state.taskbar_created_message,
            state.appbar_registered,
        ))
    });
    let Some((callback_message, taskbar_created_message, registered)) = proxy_state else {
        return false;
    };
    if message == callback_message {
        appbar_notification(hwnd, wparam, lparam);
        return true;
    }
    if message == taskbar_created_message {
        taskbar::recover(hwnd, message);
        return true;
    }
    if registered && message == WM_WINDOWPOSCHANGED {
        appbar::window_pos_changed(hwnd);
    }
    false
}

fn cleanup_destroyed_window(hwnd: HWND) {
    if let Err(error) = super::hit_test::uninstall() {
        tracing::error!(%error, "failed to release Windows input shaping while destroying the bar window");
    }
    let _calls = match APPBAR_CALLS.lock() {
        Ok(calls) => calls,
        Err(poisoned) => {
            tracing::warn!(
                "Windows AppBar call lock is poisoned; attempting best-effort cleanup for the destroyed bar window"
            );
            poisoned.into_inner()
        }
    };
    if let Err(error) = clear_requested_reservation(hwnd) {
        tracing::error!(%error, "failed to release Windows AppBar while destroying the bar window");
    }
    if let Err(error) = destroy_proxy() {
        tracing::error!(%error, "failed to destroy the Windows AppBar proxy with the bar window");
    }
}

fn forget_destroyed_window(hwnd: HWND) {
    if let Ok(mut state) = state().lock()
        && state.hwnd == Some(raw_hwnd(hwnd))
    {
        *state = NativeState::default();
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _reference_data: usize,
) -> LRESULT {
    if let Some(result) = super::hit_test::window_message(hwnd, message) {
        return result;
    }

    if message == WM_DPICHANGED {
        // Let Tauri resize the HWND for the new DPI before recomputing the reservation.
        // SAFETY: forwarding untouched message parameters is required by the subclass contract.
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if let Some(appbar_hwnd) = appbar_for_host(hwnd) {
            reassert_appbar(
                appbar_hwnd,
                "Windows AppBar reservation reapplied after DPI change",
            );
        }
        return result;
    }

    if message == WM_ACTIVATE {
        notify_activation(hwnd);
    }
    if message == WM_DISPLAYCHANGE
        && let Ok(state) = lock_state()
        && let Some(app) = state.app.clone()
    {
        super::monitor::schedule_refresh(app);
    }

    super::placement::window_message(hwnd, message, lparam);

    if message == WM_DESTROY {
        cleanup_destroyed_window(hwnd);
    } else if message == WM_NCDESTROY {
        forget_destroyed_window(hwnd);
        // SAFETY: this callback and ID were installed on this HWND in install().
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
    }

    // SAFETY: every unhandled message must continue through the existing subclass chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
