use windows::Win32::{
    Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GW_HWNDPREV, GetWindow, HWND_BOTTOM,
        HWND_NOTOPMOST, HWND_TOPMOST, RegisterClassW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowPos, WINDOWPOS, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_POPUP,
    },
};
use windows::core::{PCWSTR, w};

struct Window(HWND);

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

impl Window {
    fn new(class: PCWSTR, x: i32) -> Self {
        let module = HINSTANCE(unsafe { GetModuleHandleW(None) }.expect("module").0);
        if unsafe { class.as_wide() != w!("STATIC").as_wide() } {
            let definition = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: class,
                ..Default::default()
            };
            let atom = unsafe { RegisterClassW(&definition) };
            assert!(
                atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS,
                "register test taskbar"
            );
        }
        Self(
            unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    class,
                    w!("smabar taskbar priority test"),
                    WS_POPUP,
                    x,
                    -20_000,
                    400,
                    40,
                    None,
                    None,
                    Some(module),
                    None,
                )
            }
            .expect("create isolated test window"),
        )
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) }.expect("destroy test window");
    }
}

#[test]
fn motion_yields_to_taskbars_and_preserves_lowering() {
    let primary = Window::new(w!("Shell_TrayWnd"), -20_000);
    let secondary = Window::new(w!("Shell_SecondaryTrayWnd"), -19_000);
    let bar = Window::new(w!("STATIC"), -20_000);

    for (taskbar, x) in [(primary.0, -20_000), (secondary.0, -19_000)] {
        for offset in [0, 8, 20, 32] {
            let mut pos = WINDOWPOS {
                hwnd: bar.0,
                hwndInsertAfter: HWND_TOPMOST,
                x,
                y: -20_020 + offset,
                cx: 400,
                cy: 40,
                flags: SWP_NOACTIVATE,
            };
            super::adjust(bar.0, &mut pos).expect("adjust bar motion");
            assert_eq!(pos.hwndInsertAfter, taskbar);
            assert_eq!(
                (pos.x, pos.y, pos.cx, pos.cy),
                (x, -20_020 + offset, 400, 40)
            );
            unsafe {
                SetWindowPos(
                    bar.0,
                    Some(pos.hwndInsertAfter),
                    pos.x,
                    pos.y,
                    pos.cx,
                    pos.cy,
                    pos.flags,
                )
            }
            .expect("place bar behind test taskbar");
            let mut preceding = unsafe { GetWindow(bar.0, GW_HWNDPREV) };
            let mut seen = std::collections::HashSet::new();
            while let Ok(window) = preceding {
                if window == taskbar || !seen.insert(window.0 as usize) {
                    break;
                }
                preceding = unsafe { GetWindow(window, GW_HWNDPREV) };
            }
            assert_eq!(preceding.expect("taskbar precedes the bar"), taskbar);
        }
    }
    for target in [HWND_BOTTOM, HWND_NOTOPMOST] {
        let mut pos = WINDOWPOS {
            hwnd: bar.0,
            hwndInsertAfter: target,
            flags: SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            ..Default::default()
        };
        super::adjust(bar.0, &mut pos).expect("preserve lowering");
        assert_eq!(pos.hwndInsertAfter, target);
    }
    let mut pos = WINDOWPOS {
        hwnd: bar.0,
        hwndInsertAfter: HWND_TOPMOST,
        x: -18_000,
        y: -20_000,
        cx: 400,
        cy: 40,
        flags: SWP_NOACTIVATE,
    };
    super::adjust(bar.0, &mut pos).expect("no overlapping taskbar");
    assert_eq!(pos.hwndInsertAfter, HWND_TOPMOST);

    // A replacement HWND is discovered on the next placement, without a cache.
    drop(secondary);
    let replacement = Window::new(w!("Shell_SecondaryTrayWnd"), -19_000);
    let mut pos = WINDOWPOS {
        hwnd: bar.0,
        flags: SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        ..Default::default()
    };
    super::adjust(bar.0, &mut pos).expect("replacement Explorer taskbar");
    assert_eq!(pos.hwndInsertAfter, replacement.0);
    assert!(!pos.flags.contains(SWP_NOZORDER));
    drop(replacement);
}
