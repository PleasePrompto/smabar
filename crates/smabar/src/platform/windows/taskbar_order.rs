//! Keep the moving bar behind any system taskbar it overlaps.

use anyhow::Context;
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect, HWND_BOTTOM,
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetWindowPos, WINDOWPOS, WS_EX_TOPMOST,
    },
};
use windows::core::{BOOL, w};

struct Search {
    rect: RECT,
    taskbar: Option<HWND>,
}

unsafe extern "system" fn find_taskbar(hwnd: HWND, param: LPARAM) -> BOOL {
    // SAFETY: EnumWindows synchronously borrows the Search passed by adjust().
    let search = unsafe { &mut *(param.0 as *mut Search) };
    if search.taskbar.is_some() {
        return true.into();
    }
    let mut class = [0_u16; 64];
    let count = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    if !is_taskbar_class(&class[..count]) {
        return true.into();
    }
    let mut rect = RECT::default();
    // A taskbar can disappear during an Explorer restart; it is then no target.
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok()
        && rect.left < search.rect.right
        && rect.right > search.rect.left
        && rect.top < search.rect.bottom
        && rect.bottom > search.rect.top
    {
        search.taskbar = Some(hwnd);
    }
    true.into()
}

fn is_taskbar_class(class: &[u16]) -> bool {
    let primary = w!("Shell_TrayWnd");
    let secondary = w!("Shell_SecondaryTrayWnd");
    // SAFETY: both constants are static, nul-terminated UTF-16 strings.
    unsafe { class == primary.as_wide() || class == secondary.as_wide() }
}

pub(super) fn adjust(hwnd: HWND, pos: &mut WINDOWPOS) -> anyhow::Result<()> {
    let changes_order = !pos.flags.contains(SWP_NOZORDER);
    if changes_order && matches!(pos.hwndInsertAfter, HWND_BOTTOM | HWND_NOTOPMOST) {
        return Ok(());
    }
    let topmost = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TOPMOST.0 as isize != 0;
    if !(topmost || changes_order && pos.hwndInsertAfter == HWND_TOPMOST) {
        return Ok(());
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .context("failed to read the bar frame for taskbar priority")?;
    let width = if pos.flags.contains(SWP_NOSIZE) {
        rect.right - rect.left
    } else {
        pos.cx
    };
    let height = if pos.flags.contains(SWP_NOSIZE) {
        rect.bottom - rect.top
    } else {
        pos.cy
    };
    if !pos.flags.contains(SWP_NOMOVE) {
        rect.left = pos.x;
        rect.top = pos.y;
    }
    rect.right = rect.left.saturating_add(width);
    rect.bottom = rect.top.saturating_add(height);
    let mut search = Search {
        rect,
        taskbar: None,
    };
    // SAFETY: Search lives throughout this synchronous enumeration; the callback
    // only reads native window metadata and never sends messages to Explorer.
    unsafe {
        EnumWindows(
            Some(find_taskbar),
            LPARAM(&mut search as *mut Search as isize),
        )
    }
    .context("failed to find the system taskbar")?;
    if let Some(taskbar) = search.taskbar {
        pos.hwndInsertAfter = taskbar;
        pos.flags &= !SWP_NOZORDER;
    }
    Ok(())
}

pub(super) fn refresh(hwnd: HWND) -> anyhow::Result<()> {
    // Let the host's WM_WINDOWPOSCHANGING handler restore priority after Explorer
    // replaces its HWND. No cached taskbar handle, movement, or activation.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .context("failed to restore system taskbar priority after Explorer restart")
}

#[cfg(test)]
#[path = "taskbar_order_tests.rs"]
mod tests;
