//! Region-aware Windows hit testing fed by the shell's CSS rectangles.

use smabar_core::platform::Rect;
use tauri::WebviewWindow;

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
