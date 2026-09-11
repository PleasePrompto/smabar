use std::{
    mem::size_of,
    os::windows::io::AsRawHandle,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, anyhow, bail};
#[cfg(debug_assertions)]
use windows::Win32::UI::WindowsAndMessaging::WM_APP;
use windows::Win32::{
    Foundation::{
        ERROR_INVALID_HOOK_HANDLE, HANDLE, HINSTANCE, LPARAM, LRESULT, POINT, WAIT_FAILED,
        WAIT_OBJECT_0, WPARAM,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentThreadId, INFINITE, WaitForSingleObject},
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, HHOOK, KillTimer, MSG,
            MSLLHOOKSTRUCT, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_NOREMOVE,
            PM_QS_SENDMESSAGE, PeekMessageW, PostThreadMessageW, QS_SENDMESSAGE, SetTimer,
            SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WM_QUIT,
            WM_TIMER,
        },
    },
};

use crate::platform::windows_watchdog::{
    Action, Observation, POLL_INTERVAL_MS, Point as WatchdogPoint, Watchdog,
};

static HOOK_TICK: AtomicU32 = AtomicU32::new(0);
static HOOK_GENERATION: AtomicU64 = AtomicU64::new(0);
const SAMPLE_LOG_COOLDOWN_TICKS: u16 = (5 * 60_000 / POLL_INTERVAL_MS) as u16;

#[cfg(debug_assertions)]
pub(super) const DEBUG_DROP_HOOK_MESSAGE: u32 = WM_APP + 0x534;

pub(super) struct HookThread {
    id: u32,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct SampleLogging {
    failed: bool,
    cooldown_ticks: u16,
    incident_reported: bool,
}

impl SampleLogging {
    fn tick(&mut self) {
        self.cooldown_ticks = self.cooldown_ticks.saturating_sub(1);
    }

    fn failure(&mut self, error: &anyhow::Error) {
        if !self.failed && self.cooldown_ticks == 0 {
            self.cooldown_ticks = SAMPLE_LOG_COOLDOWN_TICKS;
            self.incident_reported = true;
            tracing::error!(
                %error,
                "Windows mouse-hook watchdog could not sample session input; the existing routing state is retained, but silent hook loss cannot be detected until sampling recovers"
            );
        }
        self.failed = true;
    }

    fn success(&mut self) {
        if self.failed && std::mem::take(&mut self.incident_reported) {
            tracing::info!("Windows mouse-hook watchdog input sampling recovered");
        }
        self.failed = false;
    }
}

impl HookThread {
    pub(super) fn id(&self) -> u32 {
        self.id
    }
}

pub(super) fn start() -> anyhow::Result<HookThread> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let handle = thread::Builder::new()
        .name("smabar-windows-mouse-hook".to_string())
        .spawn(move || hook_thread(ready_tx))
        .context("failed to start the Windows input-shape mouse-hook thread")?;
    let ready = match ready_rx.recv() {
        Ok(ready) => ready,
        Err(error) => {
            let _ = handle.join();
            return Err(error)
                .context("Windows input-shape mouse-hook thread stopped during startup");
        }
    };
    match ready {
        Ok(id) => Ok(HookThread { id, handle }),
        Err(error) => {
            let _ = handle.join();
            bail!("failed to install the Windows input-shape mouse hook: {error}");
        }
    }
}

pub(super) fn stop(hook: HookThread) -> (bool, Option<anyhow::Error>) {
    let HookThread { id, handle } = hook;
    let mut failure = None;
    let stopped = match unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        .context("failed to stop the Windows input-shape mouse-hook thread")
    {
        Ok(()) => match wait_for_hook(&handle) {
            Ok(()) => true,
            Err(error) => {
                failure = Some(error);
                false
            }
        },
        Err(error) => {
            failure = Some(error);
            (unsafe { WaitForSingleObject(thread_handle(&handle), 0) }) == WAIT_OBJECT_0
        }
    };
    if stopped && handle.join().is_err() && failure.is_none() {
        failure = Some(anyhow!("Windows input-shape mouse-hook thread panicked"));
    }
    (stopped, failure)
}

pub(super) fn wait_for_hook(handle: &JoinHandle<()>) -> anyhow::Result<()> {
    let handle = thread_handle(handle);
    loop {
        let status = unsafe {
            MsgWaitForMultipleObjectsEx(
                Some(&[handle]),
                INFINITE,
                QS_SENDMESSAGE,
                MWMO_INPUTAVAILABLE,
            )
        };
        if status == WAIT_OBJECT_0 {
            return Ok(());
        }
        if status == WAIT_FAILED {
            return Err(windows::core::Error::from_thread())
                .context("failed while waiting for the Windows mouse-hook thread");
        }
        if status.0 == WAIT_OBJECT_0.0 + 1 {
            let mut message = MSG::default();
            // PeekMessage dispatches pending cross-thread sent messages before
            // examining the queue, which lets an in-flight style write finish.
            let _ =
                unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE) };
            continue;
        }
        bail!("unexpected Windows mouse-hook wait result {}", status.0);
    }
}

fn thread_handle(handle: &JoinHandle<()>) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

fn hook_thread(ready: mpsc::SyncSender<Result<u32, String>>) {
    let module = match unsafe { GetModuleHandleW(None) }.map(|module| HINSTANCE(module.0)) {
        Ok(module) => module,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let mut hook = match install_hook(module) {
        Ok(hook) => Some(hook),
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };

    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    let timer = unsafe { SetTimer(None, 0, POLL_INTERVAL_MS, None) };
    if timer == 0 {
        let _ = release_hook(&mut hook);
        let error = anyhow::Error::new(windows::core::Error::from_thread())
            .context("failed to start the Windows mouse-hook watchdog timer");
        let _ = ready.send(Err(error.to_string()));
        return;
    }
    let mut sample_logging = SampleLogging::default();
    let mut watchdog = initial_watchdog(&mut sample_logging);
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(thread_id)).is_err() {
        let _ = unsafe { KillTimer(None, timer) };
        let _ = release_hook(&mut hook);
        return;
    }

    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        if status <= 0 {
            if status < 0 {
                tracing::error!(
                    error = %windows::core::Error::from_thread(),
                    "Windows input-shape mouse-hook message loop failed"
                );
            }
            break;
        }
        if message.message == WM_TIMER && message.wParam.0 == timer {
            poll_watchdog(module, &mut hook, &mut watchdog, &mut sample_logging);
            continue;
        }
        #[cfg(debug_assertions)]
        if message.message == DEBUG_DROP_HOOK_MESSAGE {
            match release_hook(&mut hook) {
                Ok(true) => tracing::info!(
                    message = DEBUG_DROP_HOOK_MESSAGE,
                    "debug trigger removed the Windows mouse hook; the watchdog must restore it"
                ),
                Ok(false) => tracing::warn!(
                    message = DEBUG_DROP_HOOK_MESSAGE,
                    "debug trigger found no installed Windows mouse hook"
                ),
                Err(error) => tracing::error!(
                    %error,
                    message = DEBUG_DROP_HOOK_MESSAGE,
                    "debug trigger could not remove the Windows mouse hook"
                ),
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    if let Err(error) = unsafe { KillTimer(None, timer) } {
        tracing::error!(%error, "failed to stop the Windows mouse-hook watchdog timer");
    }
    if let Err(error) = release_hook(&mut hook) {
        tracing::error!(%error, "failed to release the Windows input-shape mouse hook");
    }
}

fn install_hook(module: HINSTANCE) -> windows::core::Result<HHOOK> {
    unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module), 0) }
}

fn initial_watchdog(sample_logging: &mut SampleLogging) -> Option<Watchdog> {
    match sample_observation() {
        Ok(observation) => Some(Watchdog::new(observation)),
        Err(error) => {
            sample_logging.failure(&error);
            None
        }
    }
}

fn poll_watchdog(
    module: HINSTANCE,
    hook: &mut Option<HHOOK>,
    watchdog: &mut Option<Watchdog>,
    sample_logging: &mut SampleLogging,
) {
    sample_logging.tick();
    let current = match sample_observation() {
        Ok(current) => {
            sample_logging.success();
            current
        }
        Err(error) => {
            sample_logging.failure(&error);
            return;
        }
    };
    let Some(watchdog) = watchdog else {
        *watchdog = Some(Watchdog::new(current));
        return;
    };
    match watchdog.observe(current) {
        Action::None => {}
        Action::Recovered { report_recovery } => {
            if report_recovery {
                tracing::info!(
                    "Windows regional click-through recovered after the mouse hook resumed reporting"
                );
            }
        }
        Action::Reinstall { report_loss } => {
            reinstall_hook(module, hook, watchdog, current, report_loss);
        }
    }
}

fn reinstall_hook(
    module: HINSTANCE,
    hook: &mut Option<HHOOK>,
    watchdog: &mut Watchdog,
    current: Observation,
    report_loss: bool,
) {
    let mut failure = match super::mark_hook_lost() {
        Ok(true) => None,
        Ok(false) => return,
        Err(error) => Some(error.context("failed to hold the Windows bar fully interactive")),
    };
    if let Err(error) = release_hook(hook) {
        if failure.is_none() {
            failure = Some(error);
        }
        let retry_delay_ms = watchdog.finish_attempt(false, current);
        if report_loss {
            log_loss(failure, retry_delay_ms, false);
        }
        return;
    }

    let installed = match install_hook(module) {
        Ok(replacement) => {
            *hook = Some(replacement);
            match super::mark_hook_reinstalled() {
                Ok(true) => true,
                Ok(false) => {
                    let _ = release_hook(hook);
                    return;
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error.context(
                            "failed to restore regional click-through after reinstalling the Windows mouse hook",
                        ));
                    }
                    let _ = super::mark_hook_lost();
                    if let Err(cleanup) = release_hook(hook)
                        && failure.is_none()
                    {
                        failure = Some(cleanup);
                    }
                    false
                }
            }
        }
        Err(error) => {
            if failure.is_none() {
                failure = Some(
                    anyhow::Error::new(error)
                        .context("failed to reinstall the Windows input-shape mouse hook"),
                );
            }
            false
        }
    };
    let retry_delay_ms = watchdog.finish_attempt(installed, current);
    if report_loss {
        log_loss(failure, retry_delay_ms, installed);
    }
}

fn log_loss(failure: Option<anyhow::Error>, retry_delay_ms: u32, replacement_installed: bool) {
    match failure {
        Some(error) => tracing::error!(
            %error,
            retry_delay_ms,
            replacement_installed,
            "Windows input continued while the mouse hook stopped reporting and recovery encountered an error; smabar falls back to a fully interactive bar whenever the hook is unavailable and will retry automatically. If this persists, restart smabar and allow its global mouse hook in antivirus or anti-cheat software"
        ),
        None => tracing::warn!(
            retry_delay_ms,
            "Windows input continued while the mouse hook stopped reporting; smabar held the bar fully interactive and reinstalled the hook. If this repeats, allow smabar's global mouse hook in antivirus or anti-cheat software"
        ),
    }
}

fn release_hook(hook: &mut Option<HHOOK>) -> anyhow::Result<bool> {
    let Some(installed) = *hook else {
        return Ok(false);
    };
    match unsafe { UnhookWindowsHookEx(installed) } {
        Ok(()) => {
            *hook = None;
            Ok(true)
        }
        Err(error) if error.code() == ERROR_INVALID_HOOK_HANDLE.to_hresult() => {
            *hook = None;
            Ok(true)
        }
        Err(error) => {
            Err(error).context("failed to remove the existing Windows input-shape mouse hook")
        }
    }
}

fn sample_observation() -> anyhow::Result<Observation> {
    let mut input = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe { GetLastInputInfo(&mut input) }
        .ok()
        .context("failed to read the Windows session's last-input marker")?;
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }
        .context("failed to read the pointer for the Windows mouse-hook watchdog")?;
    Ok(Observation {
        input_tick: input.dwTime,
        cursor: WatchdogPoint {
            x: cursor.x,
            y: cursor.y,
        },
        hook_tick: HOOK_TICK.load(Ordering::Relaxed),
        hook_generation: HOOK_GENERATION.load(Ordering::Relaxed),
    })
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let data = lparam.0 as *const MSLLHOOKSTRUCT;
        if let Some(data) = unsafe { data.as_ref() } {
            HOOK_TICK.store(data.time, Ordering::Relaxed);
            HOOK_GENERATION.fetch_add(1, Ordering::Relaxed);
            super::report_update(super::apply_hook_point(data.pt));
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
