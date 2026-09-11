//! Cross-process click-through for the bounded bar WebView host.

use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, anyhow, bail};
use smabar_core::platform::Rect;
use windows::Win32::{
    Foundation::{GetLastError, HWND, LRESULT, POINT, SetLastError, WIN32_ERROR},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, RegisterWindowMessageW, SendMessageW,
        SetWindowLongPtrW, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    },
};
use windows::core::w;

use crate::platform::windows_geometry::hits_rect;

use super::native::{raw_hwnd, window_scale};

static INPUT: OnceLock<Mutex<InputState>> = OnceLock::new();
static HOOK: Mutex<Option<hook::HookThread>> = Mutex::new(None);
static HOOK_ERROR_REPORTED: AtomicBool = AtomicBool::new(false);
static STYLE_MESSAGE: OnceLock<u32> = OnceLock::new();

const OWNED_STYLE_BITS: isize = (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize;

#[derive(Default)]
struct InputState {
    hwnd: Option<isize>,
    rects: Arc<[Rect]>,
    style_before_install: isize,
    hook_available: bool,
    generation: u32,
    pending: Option<InputRequest>,
}

#[derive(Clone, Copy)]
enum InputRequest {
    Point { x: i32, y: i32, generation: u32 },
    Interactive { generation: u32 },
}

impl InputRequest {
    fn generation(self) -> u32 {
        match self {
            Self::Point { generation, .. } | Self::Interactive { generation } => generation,
        }
    }
}

#[path = "hit_test/hook.rs"]
mod hook;

fn input() -> &'static Mutex<InputState> {
    INPUT.get_or_init(|| Mutex::new(InputState::default()))
}

fn lock_input() -> anyhow::Result<MutexGuard<'static, InputState>> {
    // Deliberately an error, unlike the codebase-wide into_inner recovery: a
    // poisoned InputState means a panic left `generation`/`pending` mid-update
    // in a state the HWND mouse hook keeps reading from another thread —
    // refuse rather than act on half-written input-shape data.
    input()
        .lock()
        .map_err(|_| anyhow!("Windows input-shape state is poisoned"))
}

fn register_style_message() -> anyhow::Result<u32> {
    if let Some(message) = STYLE_MESSAGE.get().copied() {
        return Ok(message);
    }
    let message = unsafe { RegisterWindowMessageW(w!("smabar.InputStyle")) };
    if message == 0 {
        return Err(windows::core::Error::from_thread())
            .context("failed to register the Windows input-style message");
    }
    let _ = STYLE_MESSAGE.set(message);
    Ok(STYLE_MESSAGE.get().copied().unwrap_or(message))
}

pub(super) fn install(hwnd: HWND) -> anyhow::Result<()> {
    let mut hook = HOOK
        .lock()
        .map_err(|_| anyhow!("Windows mouse-hook state is poisoned"))?;
    if hook.is_some() {
        bail!("Windows input-shape mouse hook is already installed");
    }

    {
        let input = lock_input()?;
        if input.hwnd.is_some() {
            bail!("Windows input shaping is already attached to another window");
        }
    }
    register_style_message()?;
    let current_style =
        read_style(hwnd).context("failed to read the Windows bar input style before installing")?;

    let hook_thread = hook::start()?;
    let id = hook_thread.id();

    let mut input = match lock_input() {
        Ok(input) => input,
        Err(mut error) => {
            drop(hook);
            let (stopped, cleanup) = hook::stop(hook_thread);
            if let Some(cleanup) = cleanup {
                error = error.context(format!(
                    "the unarmed Windows mouse hook also could not be released: {cleanup:#}"
                ));
            } else if !stopped {
                error = error.context("the unarmed Windows mouse hook did not stop");
            }
            return Err(error);
        }
    };
    input.hwnd = Some(raw_hwnd(hwnd));
    input.rects = Arc::default();
    input.style_before_install = current_style & OWNED_STYLE_BITS;
    input.hook_available = true;
    input.generation = input.generation.wrapping_add(1);
    input.pending = None;
    drop(input);
    *hook = Some(hook_thread);
    drop(hook);

    if let Err(error) = refresh_cursor() {
        if let Err(cleanup) = uninstall() {
            return Err(error).context(format!(
                "the failed Windows input-shape installation also could not be released: {cleanup:#}"
            ));
        }
        return Err(error);
    }
    tracing::info!(
        hwnd = raw_hwnd(hwnd),
        hook_thread_id = id,
        "Windows regional click-through initialized"
    );
    Ok(())
}

pub(super) fn set_rects(hwnd: HWND, rects: Vec<Rect>) -> anyhow::Result<()> {
    let (dpi, scale) = window_scale(hwnd)?;
    let rect_count = rects.len();
    {
        let mut input = lock_input()?;
        if input.hwnd != Some(raw_hwnd(hwnd)) {
            bail!("Windows input shaping is not installed for this window");
        }
        input.rects = rects.into();
        input.generation = input.generation.wrapping_add(1);
        input.pending = None;
    }
    apply_current_cursor()?;
    tracing::info!(
        hwnd = raw_hwnd(hwnd),
        rect_count,
        dpi,
        scale_factor = scale,
        "Windows input shape applied"
    );
    Ok(())
}

pub(super) fn uninstall() -> anyhow::Result<()> {
    // Stop new callbacks from touching the HWND before waiting for the hook.
    // An already-running style change can synchronously call the owner thread,
    // so wait_for_hook pumps sent messages instead of blindly joining here.
    let (raw, style_before_install) = {
        let mut input = lock_input()?;
        let snapshot = (input.hwnd.take(), input.style_before_install);
        input.rects = Arc::default();
        input.style_before_install = 0;
        input.hook_available = false;
        input.generation = input.generation.wrapping_add(1);
        input.pending = None;
        snapshot
    };
    let hook = HOOK
        .lock()
        .map_err(|_| anyhow!("Windows mouse-hook state is poisoned"))?
        .take();
    let (hook_stopped, mut failure) = hook.map_or((true, None), hook::stop);

    if !hook_stopped {
        return Err(failure.unwrap_or_else(|| {
            anyhow!(
                "Windows input-shape mouse-hook thread did not stop; its style was not restored"
            )
        }));
    }

    if let Some(raw) = raw {
        let hwnd = HWND(raw as _);
        match read_style(hwnd) {
            Ok(style) => {
                let restored = restored_style(style, style_before_install);
                let mut style_restored = true;
                if restored != style
                    && let Err(error) = write_style(hwnd, restored)
                        .context("failed to restore the Windows bar input style")
                {
                    style_restored = false;
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
                if style_restored {
                    tracing::info!(hwnd = raw, "Windows regional click-through released");
                }
            }
            Err(error) if failure.is_none() => {
                failure = Some(
                    error.context("failed to read the Windows bar input style before restoring it"),
                );
            }
            Err(_) => {}
        }
    }
    HOOK_ERROR_REPORTED.store(false, Ordering::Relaxed);
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) fn refresh_cursor() -> anyhow::Result<()> {
    {
        let mut input = lock_input()?;
        if input.hwnd.is_some() {
            input.generation = input.generation.wrapping_add(1);
            input.pending = None;
        }
    }
    apply_current_cursor()
}

fn apply_current_cursor() -> anyhow::Result<()> {
    let result = (|| {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }
            .context("failed to read the Windows pointer position")?;
        apply_owner_point(point).context("failed to update Windows regional click-through")
    })();
    if let Err(error) = result {
        return match hold_fully_interactive() {
            Ok(()) => Err(error.context(
                "Windows pointer routing could not be refreshed; smabar was held fully interactive instead of retaining stale click-through",
            )),
            Err(fallback) => Err(error.context(format!(
                "Windows pointer routing could not be refreshed and smabar could not force fully interactive input: {fallback:#}"
            ))),
        };
    }
    Ok(())
}

fn hold_fully_interactive() -> anyhow::Result<()> {
    let raw = lock_input()?.hwnd;
    match raw {
        Some(raw) => set_transparent(HWND(raw as _), false),
        None => Ok(()),
    }
}

fn apply_owner_point(mut point: POINT) -> anyhow::Result<()> {
    let Some((raw, rects, hook_available)) = ({
        let input = lock_input()?;
        input
            .hwnd
            .map(|raw| (raw, Arc::clone(&input.rects), input.hook_available))
    }) else {
        return Ok(());
    };
    let hwnd = HWND(raw as _);
    if !hook_available {
        return set_transparent(hwnd, false);
    }
    unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point) }
        .ok()
        .context("failed to convert the Windows pointer to bar coordinates")?;
    let (_, scale) = window_scale(hwnd)?;
    set_transparent(hwnd, !hits_rect(&rects, point.x, point.y, scale))
}

fn apply_hook_point(point: POINT) -> anyhow::Result<()> {
    let Some((raw, rects, generation)) = ({
        let input = lock_input()?;
        if input.hook_available {
            input
                .hwnd
                .map(|raw| (raw, Arc::clone(&input.rects), input.generation))
        } else {
            None
        }
    }) else {
        return Ok(());
    };
    let hwnd = HWND(raw as _);
    let mut client_point = point;
    unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut client_point) }
        .ok()
        .context("failed to convert the Windows pointer to bar coordinates")?;
    let (_, scale) = window_scale(hwnd)?;
    let transparent = !hits_rect(&rects, client_point.x, client_point.y, scale);
    let current = read_style(hwnd)?;
    if desired_style(current, transparent) == current {
        return Ok(());
    }

    let _ = send_owner_request(
        raw,
        InputRequest::Point {
            x: point.x,
            y: point.y,
            generation,
        },
    )?;
    Ok(())
}

pub(super) fn mark_hook_lost() -> anyhow::Result<bool> {
    let Some((raw, generation)) = ({
        let mut input = lock_input()?;
        input.hwnd.map(|raw| {
            input.hook_available = false;
            input.generation = input.generation.wrapping_add(1);
            input.pending = None;
            (raw, input.generation)
        })
    }) else {
        return Ok(false);
    };
    let _ = send_owner_request(raw, InputRequest::Interactive { generation })?;
    Ok(lock_input()?.hwnd == Some(raw))
}

pub(super) fn mark_hook_reinstalled() -> anyhow::Result<bool> {
    let raw = {
        let mut input = lock_input()?;
        let Some(raw) = input.hwnd else {
            return Ok(false);
        };
        input.hook_available = true;
        input.generation = input.generation.wrapping_add(1);
        input.pending = None;
        raw
    };
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }
        .context("failed to read the pointer while restoring the Windows mouse hook")?;
    apply_hook_point(point)?;
    Ok(lock_input()?.hwnd == Some(raw))
}

fn send_owner_request(raw: isize, request: InputRequest) -> anyhow::Result<bool> {
    {
        let mut input = lock_input()?;
        if input.hwnd != Some(raw) || input.generation != request.generation() {
            return Ok(false);
        }
        input.pending = Some(request);
    }
    let message = STYLE_MESSAGE
        .get()
        .copied()
        .context("Windows input-style message is not registered")?;
    if unsafe { SendMessageW(HWND(raw as _), message, None, None) }.0 == 0 {
        bail!("Windows bar did not process the input-style update message");
    }
    let input = lock_input()?;
    Ok(input.hwnd == Some(raw) && input.generation == request.generation())
}

pub(super) fn window_message(hwnd: HWND, message: u32) -> Option<LRESULT> {
    if STYLE_MESSAGE.get().copied() != Some(message) {
        return None;
    }
    let result = (|| {
        let request = {
            let mut input = lock_input()?;
            let Some(request) = input.pending.take() else {
                return Ok(());
            };
            if input.hwnd != Some(raw_hwnd(hwnd)) || request.generation() != input.generation {
                return Ok(());
            }
            request
        };
        match request {
            InputRequest::Point { x, y, .. } => apply_owner_point(POINT { x, y }),
            InputRequest::Interactive { .. } => set_transparent(hwnd, false),
        }
    })();
    let succeeded = result.is_ok();
    report_update(result);
    Some(LRESULT(if succeeded { 1 } else { 0 }))
}

fn set_transparent(hwnd: HWND, transparent: bool) -> anyhow::Result<()> {
    let current = read_style(hwnd)?;
    let desired = desired_style(current, transparent);
    if desired != current {
        write_style(hwnd, desired)?;
    }
    Ok(())
}

fn desired_style(current: isize, transparent: bool) -> isize {
    let mut desired = current | WS_EX_LAYERED.0 as isize;
    if transparent {
        desired |= WS_EX_TRANSPARENT.0 as isize;
    } else {
        desired &= !(WS_EX_TRANSPARENT.0 as isize);
    }
    desired
}

fn restored_style(current: isize, original_owned_bits: isize) -> isize {
    (current & !OWNED_STYLE_BITS) | original_owned_bits
}

fn read_style(hwnd: HWND) -> anyhow::Result<isize> {
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let error = unsafe { GetLastError() };
    if style == 0 && error != WIN32_ERROR(0) {
        return Err(windows::core::Error::from_hresult(
            windows::core::HRESULT::from_win32(error.0),
        ))
        .context("failed to read the Windows bar input style");
    }
    Ok(style)
}

fn write_style(hwnd: HWND, style: isize) -> anyhow::Result<()> {
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let previous = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style) };
    let error = unsafe { GetLastError() };
    if previous == 0 && error != WIN32_ERROR(0) {
        return Err(windows::core::Error::from_hresult(
            windows::core::HRESULT::from_win32(error.0),
        ))
        .context("failed to change the Windows bar input style");
    }
    Ok(())
}

fn report_update(result: anyhow::Result<()>) {
    match result {
        Ok(()) if HOOK_ERROR_REPORTED.swap(false, Ordering::Relaxed) => {
            tracing::info!(
                "Windows regional click-through recovered after a mouse-hook update failure"
            );
        }
        Ok(()) => {}
        Err(error) if !HOOK_ERROR_REPORTED.swap(true, Ordering::Relaxed) => {
            tracing::error!(
                %error,
                "Windows regional click-through update failed; pointer input may be routed incorrectly until the next mouse event"
            );
        }
        Err(_) => {}
    }
}

#[cfg(test)]
#[path = "hit_test_tests.rs"]
mod tests;
