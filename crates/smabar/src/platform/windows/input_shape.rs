//! Region-aware Windows hit testing fed by the shell's CSS rectangles.

use anyhow::Context;
use smabar_core::platform::Rect;
use tauri::WebviewWindow;
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::ScreenToClient,
    UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetCursorPos, WindowFromPoint},
};

/// Use the same screen point for hit-testing and conversion. WebView2 may
/// own the child HWND beneath the pointer, so compare its native root.
pub fn pointer_sample(window: &WebviewWindow) -> anyhow::Result<Option<[f64; 2]>> {
    let hwnd = super::native::window_hwnd(window)?;
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.context("failed to sample the Windows pointer")?;
    if unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) } != hwnd {
        return Ok(None);
    }
    unsafe { ScreenToClient(hwnd, &mut point) }
        .ok()
        .context("failed to convert the Windows pointer to bar coordinates")?;
    let scale = window.scale_factor()?;
    Ok(Some([
        f64::from(point.x) / scale,
        f64::from(point.y) / scale,
    ]))
}

pub fn apply(window: &WebviewWindow, rects: Vec<Rect>) -> anyhow::Result<()> {
    let hwnd = super::native::window_hwnd(window)?;
    super::hit_test::set_rects(hwnd, rects)
}

#[tauri::command]
pub fn set_input_shape(window: WebviewWindow, rects: Vec<Rect>) -> Result<(), String> {
    let bar = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(error) = apply(&bar, rects) {
                tracing::error!(
                    %error,
                    "failed to update the Windows input shape; pointer input may be routed incorrectly"
                );
            }
        })
        .map_err(|error| format!("failed to schedule the Windows input-shape update: {error}"))
}
