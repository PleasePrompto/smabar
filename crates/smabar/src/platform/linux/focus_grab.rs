//! Hyprland reports outside clicks independently of keyboard focus.

use std::ffi::{c_int, c_void};

use anyhow::Context;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::Cast;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::surfaces::{SurfaceManager, SurfaceRole};

unsafe extern "C" {
    fn smabar_focus_grab_release(window: *mut gtk::ffi::GtkWindow);
    fn smabar_focus_grab_install(
        window: *mut gtk::ffi::GtkWindow,
        context: *mut c_void,
        cleared: unsafe extern "C" fn(*mut c_void, u64),
        free_context: unsafe extern "C" fn(*mut c_void),
    ) -> c_int;
    fn smabar_focus_grab_activate(
        window: *mut gtk::ffi::GtkWindow,
        bar: *mut gtk::ffi::GtkWindow,
        generation: u64,
    ) -> c_int;
}

pub(super) fn release(window: &gtk::ApplicationWindow) {
    // SAFETY: the caller owns this GTK window on the main thread. The bridge
    // only drops its optional grab; it does not retain or destroy the window.
    unsafe { smabar_focus_grab_release(window.upcast_ref::<gtk::Window>().to_glib_none().0) };
}

pub(super) fn install(window: &WebviewWindow) -> anyhow::Result<()> {
    let native = window.gtk_window()?;
    let context = Box::into_raw(Box::new(window.app_handle().clone())).cast();
    // SAFETY: setup runs on GTK's main thread. C owns this AppHandle until
    // the GTK window is finalized, and calls both callbacks on that thread.
    let result = unsafe {
        smabar_focus_grab_install(
            native.upcast_ref::<gtk::Window>().to_glib_none().0,
            context,
            cleared,
            free_context,
        )
    };
    anyhow::ensure!(result >= 0, "failed to discover Wayland focus-grab support");
    tracing::info!(
        supported = result == 1,
        "Wayland outside-click dismissal initialized"
    );
    Ok(())
}

pub(super) fn activate(window: &WebviewWindow) -> anyhow::Result<()> {
    let bar = window
        .app_handle()
        .get_webview_window(SurfaceRole::Bar.label())
        .context("cannot activate overlay dismissal without the bar")?
        .gtk_window()?;
    let native = window.gtk_window()?;
    let generation = window
        .app_handle()
        .state::<SurfaceManager>()
        .overlay_generation()?;
    // SAFETY: both borrowed windows remain alive on GTK's main thread. C
    // obtains their current wl_surfaces and releases the grab on overlay unmap.
    let result = unsafe {
        smabar_focus_grab_activate(
            native.upcast_ref::<gtk::Window>().to_glib_none().0,
            bar.upcast_ref::<gtk::Window>().to_glib_none().0,
            generation,
        )
    };
    anyhow::ensure!(
        result >= 0,
        "cannot activate Wayland dismissal before the bar and overlay are mapped"
    );
    Ok(())
}

unsafe extern "C" fn cleared(context: *mut c_void, generation: u64) {
    // SAFETY: install transfers this box to C; it remains alive until free_context.
    let app = unsafe { &*context.cast::<AppHandle>() };
    if let Err(error) = app
        .state::<SurfaceManager>()
        .dismiss_overlay(app, Some(generation))
    {
        tracing::error!(%error, "failed to dismiss overlay after an outside click; press Escape to close it");
    }
}

unsafe extern "C" fn free_context(context: *mut c_void) {
    // SAFETY: C invokes this exactly once for the box transferred by install.
    drop(unsafe { Box::from_raw(context.cast::<AppHandle>()) });
}
