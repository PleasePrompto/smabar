use anyhow::Context;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::MonitorExt;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use smabar_core::platform::WindowLevel;
use tauri::{Manager, WebviewWindow};

use crate::surfaces::SurfaceRole;

/// GTK can stop receiving frame callbacks for an autohidden surface. Commit
/// pending layer geometry through the native API that handles GTK's state.
pub(super) fn commit(window: &impl gtk::prelude::IsA<gtk::Window>) {
    type Commit = unsafe extern "C" fn(*mut gtk::ffi::GtkWindow);
    static COMMIT: std::sync::OnceLock<Option<Commit>> = std::sync::OnceLock::new();
    let commit = COMMIT.get_or_init(|| {
        // SAFETY: layer-shell is already linked for the process lifetime. The
        // 0.9 symbol has this exact C signature but no GTK3 Rust binding. An
        // optional lookup preserves startup on older distributions and X11.
        let symbol = unsafe {
            libc::dlsym(libc::RTLD_DEFAULT, c"gtk_layer_try_force_commit".as_ptr())
        };
        if symbol.is_null() {
            tracing::warn!("gtk-layer-shell 0.9 or newer is needed for prompt hidden-surface movement; upgrade gtk-layer-shell");
            None
        } else {
            // SAFETY: the non-null symbol's signature and lifetime are above.
            Some(unsafe { std::mem::transmute::<*mut libc::c_void, Commit>(symbol) })
        }
    });
    if let Some(commit) = commit {
        // SAFETY: callers mutate layer surfaces on GTK's main thread; the
        // borrowed GtkWindow remains alive throughout this synchronous call.
        unsafe { commit(window.as_ref().to_glib_none().0) };
    }
}

pub(super) fn place_transient(
    window: &WebviewWindow,
    native: &gtk::ApplicationWindow,
    x: i32,
    y: i32,
    height: i32,
    keyboard: bool,
) -> anyhow::Result<()> {
    let monitor = native
        .monitor()
        .context("Wayland transient has no monitor")?;
    let bar = window
        .app_handle()
        .get_webview_window(SurfaceRole::Bar.label())
        .context("Wayland transient has no bar")?
        .gtk_window()?;
    let bar_at_top = bar.is_anchor(Edge::Top);
    let monitor_height = monitor.geometry().height();
    let top = if window.label() == SurfaceRole::Overlay.label() {
        bar_at_top
    } else {
        y + height / 2 < monitor_height / 2
    };
    let reserved = if top == bar_at_top {
        super::reservation::height()
    } else {
        0
    };
    let margin = if top { y } else { monitor_height - y - height } - reserved;
    // Both surfaces avoid other panels. The compositor also avoids our bar's
    // reservation for the transient, so subtract that already-budgeted space.
    native.set_anchor(Edge::Top, top);
    native.set_anchor(Edge::Bottom, !top);
    native.set_layer_shell_margin(Edge::Left, x);
    native.set_layer_shell_margin(Edge::Top, if top { margin } else { 0 });
    native.set_layer_shell_margin(Edge::Bottom, if top { 0 } else { margin });
    if !keyboard {
        super::focus_grab::release(native);
    }
    native.set_keyboard_mode(if keyboard {
        KeyboardMode::OnDemand
    } else {
        KeyboardMode::None
    });
    commit(native);
    Ok(())
}

pub(super) fn namespace(desktop: Option<&str>) -> &'static str {
    if desktop.is_some_and(|value| {
        value
            .split([':', ';'])
            .any(|name| name.eq_ignore_ascii_case("KDE"))
    }) {
        // KWin maps this scope to a dock, so Show Desktop leaves smabar visible.
        "dock"
    } else {
        // Preserve the public Hyprland namespace used by existing layer rules.
        "smabar"
    }
}

pub(super) fn for_level(level: WindowLevel) -> Layer {
    match level {
        WindowLevel::Bottom | WindowLevel::Panel => Layer::Bottom,
        WindowLevel::Top => Layer::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::namespace;

    #[test]
    fn uses_kwins_dock_scope_only_on_kde() {
        assert_eq!(namespace(Some("KDE")), "dock");
        assert_eq!(namespace(Some("GNOME:KDE")), "dock");
        assert_eq!(namespace(Some("Hyprland")), "smabar");
        assert_eq!(namespace(None), "smabar");
    }
}
