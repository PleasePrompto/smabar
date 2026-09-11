//! Thin native HWND that owns the Shell AppBar contract.

use anyhow::Context;
use windows::Win32::{
    Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, SetLastError,
        WIN32_ERROR, WPARAM,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW, WS_DISABLED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    },
};
use windows::core::w;

const CLASS_NAME: windows::core::PCWSTR = w!("smabar.AppBarProxy");

pub(super) fn create() -> anyhow::Result<HWND> {
    let module = unsafe { GetModuleHandleW(None) }
        .map(|module| HINSTANCE(module.0))
        .context("failed to resolve the Windows module for the AppBar proxy")?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: module,
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let atom = unsafe { RegisterClassW(&class) };
    let error = unsafe { GetLastError() };
    if atom == 0 && error != ERROR_CLASS_ALREADY_EXISTS {
        return Err(windows::core::Error::from_hresult(
            windows::core::HRESULT::from_win32(error.0),
        ))
        .context("failed to register the Windows AppBar proxy class");
    }

    unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!(""),
            WS_POPUP | WS_DISABLED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module),
            None,
        )
    }
    .context("failed to create the Windows AppBar proxy window")
}

pub(super) fn destroy(hwnd: HWND) -> anyhow::Result<()> {
    unsafe { DestroyWindow(hwnd) }.context("failed to destroy the Windows AppBar proxy window")
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if super::native::proxy_window_message(hwnd, message, wparam, lparam) {
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
