//! Window geometry: pin the bar window to the work area of its monitor.

use std::sync::OnceLock;

use super::input_shape;
use super::surface_monitor::resize_settings_surface;
use crate::commands::AppState;
use crate::platform::display::DisplaySnapshot;
use crate::surfaces::{SETTINGS_MIN_SIZE, SurfaceRole, clamp_settings_size};
use anyhow::Context;
use gtk::prelude::{ContainerExt, GtkWindowExt, ObjectExt, WidgetExt};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use smabar_core::config::BarPosition;
use smabar_core::platform::surfaces::{ScreenRect, offset_bar_surface};
use smabar_core::platform::{SessionKind, WindowBackend, WindowLevel};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    path::BaseDirectory,
};

pub use super::surface_monitor::{place_settings_surface, set_surface_monitor};
pub use super::transition::place_surface;

static WINDOW_BACKEND: OnceLock<WindowBackend> = OnceLock::new();
const ON_DEMAND_MIN_PROTOCOL_VERSION: u32 = 4;

pub fn backend() -> WindowBackend {
    WINDOW_BACKEND
        .get()
        .copied()
        .unwrap_or(WindowBackend::Other)
}

/// Builds one shell webview. Plugin-capable secondary views share the bar's
/// WebProcess, so splitting surfaces does not multiply Linux renderer
/// processes or provider-identity state.
pub fn create_surface(
    app: &AppHandle,
    role: SurfaceRole,
    embed_origin: Option<&str>,
    settings_size: Option<(u32, u32)>,
    settings_title: &str,
) -> anyhow::Result<WebviewWindow> {
    let extensions = app
        .path()
        .resolve("web-extensions", BaseDirectory::Resource)
        .context("failed to resolve bundled WebKit extensions")?;
    let extension = extensions.join("libsmabar-provider-identity.so");
    if !extension.is_file() {
        anyhow::bail!(
            "bundled WebKit provider-identity extension is missing at {}; reinstall smabar",
            extension.display()
        );
    }
    let window = if role == SurfaceRole::Bar {
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == role.label())
            .context("window `bar` missing from Tauri config")?;
        WebviewWindowBuilder::from_config(app, config)
            .context("invalid `bar` window config")?
            .extensions_path(extensions)
            .build()
            .context("failed to create `bar` with its WebKit provider identity")?
    } else {
        build_related_surface(app, role, extensions, settings_size, settings_title)?
    };
    super::diagnostics::observe(&window.gtk_window()?, role);
    if role.plugin_capable()
        && let Some(origin) = embed_origin
    {
        super::provider_identity::send(&window, origin)?;
    }
    Ok(window)
}

fn build_related_surface(
    app: &AppHandle,
    role: SurfaceRole,
    extensions: std::path::PathBuf,
    settings_size: Option<(u32, u32)>,
    settings_title: &str,
) -> anyhow::Result<WebviewWindow> {
    let bar = app
        .get_webview_window(SurfaceRole::Bar.label())
        .context("bar surface must exist before related surfaces")?;
    let (send, receive) =
        std::sync::mpsc::sync_channel::<gtk::glib::SendWeakRef<webkit2gtk::WebView>>(1);
    // Both steps run on GTK's main thread. Return the native weak reference
    // before build: with_webview holds the dispatcher lock, while build takes
    // the webview-map lock. Event broadcasts acquire them in the reverse order.
    bar.with_webview(move |platform| {
        let _ = send.send(platform.inner().downgrade().into());
    })
    .context("failed to reach the bar WebKit view")?;
    let related = receive
        .recv_timeout(std::time::Duration::from_secs(2))
        .context("bar WebKit view did not respond within two seconds")?
        .upgrade()
        .context("bar WebKit view was destroyed before creating the related surface")?;
    let settings_size = clamp_settings_size(settings_size);
    let builder =
        WebviewWindowBuilder::new(app, role.label(), WebviewUrl::App("index.html".into()))
            .visible(false)
            .decorations(false)
            .resizable(true)
            .extensions_path(extensions)
            .with_related_view(related);
    let builder = if role == SurfaceRole::Settings {
        // Normal top-level window; the shell draws its title bar.
        builder
            .title(settings_title)
            .transparent(false)
            .always_on_top(false)
            .skip_taskbar(false)
            .shadow(true)
            .min_inner_size(
                f64::from(SETTINGS_MIN_SIZE.0),
                f64::from(SETTINGS_MIN_SIZE.1),
            )
            .inner_size(
                f64::from(settings_size.width),
                f64::from(settings_size.height),
            )
    } else {
        builder
            .title(format!("smabar {}", role.label()))
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .inner_size(420.0, 720.0)
    };
    let window = builder.build().context("failed to build related surface")?;
    if role == SurfaceRole::Settings {
        resize_settings_surface(&window, settings_size)?;
    }
    Ok(window)
}

/// Sizes the (still hidden) bar window to exactly cover the monitor's work
/// area — the full monitor minus OS panels, in physical pixels — clears the
/// input shape so no click is eaten before the shell reports its regions,
/// and only then shows the window (avoids a misplaced first frame).
pub fn setup_surface(
    window: &WebviewWindow,
    role: SurfaceRole,
    monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    if role != SurfaceRole::Bar {
        return setup_secondary_surface(window, role, monitor);
    }

    let gtk_window = window
        .gtk_window()
        .context("failed to resolve GTK window for platform setup")?;
    let session =
        SessionKind::from_xdg_session_type(std::env::var("XDG_SESSION_TYPE").ok().as_deref());
    if session == SessionKind::Wayland {
        let entered_bar = window.clone();
        let left_bar = window.clone();
        window
            .with_webview(move |platform| {
                platform
                    .inner()
                    .connect_enter_notify_event(move |_, event| {
                        if let Err(error) = entered_bar
                            .emit("pointer-entered-input-region", event.position())
                        {
                            tracing::error!(%error, "failed to report the Wayland pointer entering the input region");
                        }
                        gtk::glib::Propagation::Proceed
                    });
                platform.inner().connect_leave_notify_event(move |_, _| {
                    if let Err(error) = left_bar.emit("pointer-left-input-region", ()) {
                        tracing::error!(%error, "failed to report the Wayland pointer leaving the input region");
                    }
                    gtk::glib::Propagation::Proceed
                });
            })
            .context("failed to install the Wayland pointer-region bridge")?;
    }
    let layer_shell_supported = session == SessionKind::Wayland && gtk_layer_shell::is_supported();
    let protocol_version = if layer_shell_supported {
        gtk_layer_shell::protocol_version()
    } else {
        0
    };
    let on_demand_keyboard_supported = protocol_version >= ON_DEMAND_MIN_PROTOCOL_VERSION;
    let mut backend =
        WindowBackend::select(session, layer_shell_supported, on_demand_keyboard_supported);
    if backend == WindowBackend::WaylandLayerShell {
        if gtk_window.is_realized() {
            tracing::warn!(
                "GTK window was realized before layer-shell initialization; running as a normal \
                 Wayland window without anchoring or reserved space. Report this Tauri lifecycle \
                 regression to the smabar maintainers"
            );
            backend = WindowBackend::WaylandFallback;
        } else {
            let position = window
                .app_handle()
                .state::<AppState>()
                .config()
                .layout
                .position;
            setup_layer_surface(&gtk_window, WindowLevel::Panel, position, monitor)?;
        }
    } else if backend == WindowBackend::WaylandFallback {
        if layer_shell_supported {
            tracing::warn!(
                protocol_version,
                required_protocol_version = ON_DEMAND_MIN_PROTOCOL_VERSION,
                "Wayland compositor's layer-shell protocol is too old for on-demand keyboard \
                 focus; running as a normal window without anchoring or reserved space. Update \
                 the compositor for full support"
            );
        } else {
            tracing::warn!(
                "Wayland compositor does not support zwlr_layer_shell_v1; running as a normal \
                 window without anchoring or reserved space. Use a layer-shell compositor such as \
                 KDE Plasma, Sway, Hyprland, or niri for full support"
            );
        }
    }
    WINDOW_BACKEND.get_or_init(|| backend);

    input_shape::apply(window, Vec::new())?;

    if backend == WindowBackend::X11 {
        // DOCK window type (set before the first map): the WM must never push
        // the bar out of its own strut-reserved strip like a normal window.
        gtk_window.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
    }

    window.show().context("failed to show bar window")?;
    Ok(())
}

fn setup_secondary_surface(
    window: &WebviewWindow,
    role: SurfaceRole,
    monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    let gtk_window = window
        .gtk_window()
        .context("failed to resolve GTK window for secondary surface setup")?;
    if backend() == WindowBackend::X11 && role != SurfaceRole::Settings {
        // Hidden NORMAL windows briefly enter Cinnamon's task list whenever
        // they are mapped again, before SKIP_TASKBAR is restored. UTILITY is
        // the native type for these app-owned surfaces and excludes them from
        // the task list from their first mapped frame. Settings is the one
        // surface that belongs in the task list and stays NORMAL.
        gtk_window.set_type_hint(gtk::gdk::WindowTypeHint::Utility);
    }
    if backend() != WindowBackend::WaylandLayerShell || role == SurfaceRole::Settings {
        return Ok(());
    }
    if gtk_window.is_realized() {
        anyhow::bail!(
            "`{}` was realized before layer-shell initialization; recreate the surface",
            role.label()
        );
    }
    gtk_window.init_layer_shell();
    let namespace = match role {
        SurfaceRole::Overlay => "smabar-overlay",
        SurfaceRole::Notifications => "smabar-notifications",
        SurfaceRole::Bar | SurfaceRole::Settings => return Ok(()),
    };
    gtk_window.set_namespace(namespace);
    gtk_window.set_layer(Layer::Overlay);
    gtk_window.set_keyboard_mode(KeyboardMode::OnDemand);
    gtk_window.set_exclusive_zone(0);
    gtk_window.set_anchor(Edge::Top, true);
    gtk_window.set_anchor(Edge::Left, true);
    let native = super::monitor::find(monitor)
        .context("selected Wayland monitor disappeared before surface setup")?;
    gtk_window.set_monitor(&native);
    if role == SurfaceRole::Overlay {
        super::focus_grab::install(window)?;
    }
    tracing::debug!(surface = role.label(), "Wayland layer surface initialized");
    Ok(())
}

pub fn place_bar_surface(
    window: &WebviewWindow,
    frame: ScreenRect,
    _monitor_origin: PhysicalPosition<i32>,
    scale: f64,
    position: BarPosition,
) -> anyhow::Result<()> {
    if backend() != WindowBackend::WaylandLayerShell {
        return place_bar_surface_x11(window, frame, scale);
    }
    let width = (f64::from(frame.w) / scale).round().max(1.0) as i32;
    let height = (f64::from(frame.h) / scale).round().max(1.0) as i32;
    let positioned = window.clone();
    window
        .run_on_main_thread(move || match positioned.gtk_window() {
            Ok(gtk_window) => {
                // Layer surfaces have no interactive resize handles. Keep GTK
                // resizable so its initial allocation cannot lock later sizes.
                gtk_window.set_resizable(true);
                gtk_window.set_size_request(width, height);
                gtk_window.resize(1, 1);
                // Hidden surfaces may not receive a frame to run GTK's layout.
                gtk_window.check_resize();
                // A single vertical anchor centers either bar width.
                gtk_window.set_anchor(Edge::Left, false);
                gtk_window.set_anchor(Edge::Right, false);
                set_vertical_anchor(&gtk_window, position);
                super::layer::commit(&gtk_window);
            }
            Err(error) => {
                tracing::error!(%error, "failed to place Wayland bar surface");
            }
        })
        .context("failed to schedule Wayland bar placement")
}

/// Size and position in ONE configure request, with the size hints sent
/// first. GTK defers both its hints and `resize` to its idle, so a separate
/// move reached the window manager while the bar still had its old height
/// and old min/max hints: a bottom bar that lost its second row first slid
/// down by that row's height and shrank a frame later — a visible jump.
fn place_bar_surface_x11(
    window: &WebviewWindow,
    frame: ScreenRect,
    scale: f64,
) -> anyhow::Result<()> {
    let width = (f64::from(frame.w) / scale).round().max(1.0) as i32;
    let height = (f64::from(frame.h) / scale).round().max(1.0) as i32;
    let x = (f64::from(frame.x) / scale).round() as i32;
    let y = (f64::from(frame.y) / scale).round() as i32;
    let placed = window.clone();
    window
        .run_on_main_thread(move || match placed.gtk_window() {
            Ok(gtk_window) => {
                // GTK locks non-resizable windows to their last allocation;
                // the bar is undecorated, so no resize UI shows meanwhile.
                gtk_window.set_resizable(true);
                gtk_window.set_size_request(width, height);
                gtk_window.resize(width, height);
                match gtk_window.window() {
                    Some(gdk_window) => {
                        gdk_window.set_geometry_hints(
                            &gtk::gdk::Geometry::new(
                                width,
                                height,
                                width,
                                height,
                                0,
                                0,
                                0,
                                0,
                                0.0,
                                0.0,
                                gtk::gdk::Gravity::NorthWest,
                            ),
                            gtk::gdk::WindowHints::MIN_SIZE | gtk::gdk::WindowHints::MAX_SIZE,
                        );
                        gdk_window.move_resize(x, y, width, height);
                    }
                    None => gtk_window.move_(x, y),
                }
                gtk_window.set_resizable(false);
                gtk_window.display().sync();
            }
            Err(error) => {
                tracing::error!(%error, "failed to place bar surface");
            }
        })
        .context("failed to schedule bar placement")
}

/// Moves the already-sized bar without changing its WebKit allocation.
pub fn move_bar_surface(
    window: &WebviewWindow,
    frame: ScreenRect,
    _monitor_origin: PhysicalPosition<i32>,
    scale: f64,
    position: BarPosition,
    offset: u32,
    at_hidden_edge: bool,
) -> anyhow::Result<()> {
    if backend() != WindowBackend::WaylandLayerShell {
        let frame = offset_bar_surface(frame, position == BarPosition::Top, offset);
        return window
            .set_position(PhysicalPosition::new(frame.x, frame.y))
            .context("failed to move bar surface");
    }
    let margin = -((f64::from(offset) / scale).round().min(f64::from(i32::MAX)) as i32);
    let moved = window.clone();
    window
        .run_on_main_thread(move || match moved.gtk_window() {
            Ok(gtk_window) => {
                // Keep the invisible activation strip at the physical edge,
                // including when another panel reserves part of the output.
                if at_hidden_edge {
                    gtk_window.set_exclusive_zone(-1);
                } else if gtk_window.exclusive_zone() < 0 {
                    gtk_window.set_exclusive_zone(0);
                }
                gtk_window.set_layer_shell_margin(Edge::Top, 0);
                gtk_window.set_layer_shell_margin(Edge::Bottom, 0);
                gtk_window.set_layer_shell_margin(
                    if position == BarPosition::Top {
                        Edge::Top
                    } else {
                        Edge::Bottom
                    },
                    margin.saturating_sub(super::reservation::height()),
                );
                super::layer::commit(&gtk_window);
            }
            Err(error) => {
                tracing::error!(%error, "failed to move Wayland bar surface");
            }
        })
        .context("failed to schedule Wayland bar movement")
}

/// Applies ordinary panel stacking or the temporary overlay level.
pub fn apply_window_level(window: &WebviewWindow, level: WindowLevel) -> anyhow::Result<()> {
    if backend() == WindowBackend::WaylandLayerShell {
        let win = window.clone();
        return window
            .run_on_main_thread(move || match win.gtk_window() {
                Ok(gtk_window) => apply_layer_shell_level(&gtk_window, level),
                Err(error) => {
                    tracing::error!(%error, "failed to resolve GTK window for layer change");
                }
            })
            .context("failed to schedule layer change on the main thread");
    }

    let flags = match (backend(), level) {
        // A DOCK without ABOVE remains above ordinary X11 windows while the
        // EWMH stacking order lets a focused fullscreen window cover it.
        (WindowBackend::X11, WindowLevel::Panel | WindowLevel::Top) => (false, false),
        (_, WindowLevel::Bottom) => (false, true),
        (_, WindowLevel::Panel | WindowLevel::Top) => (true, false),
    };
    crate::platform::set_window_stacking_flags(window, flags.0, flags.1)
}

fn apply_layer_shell_level(window: &gtk::ApplicationWindow, level: WindowLevel) {
    window.set_layer(super::layer::for_level(level));
}

pub fn focus(window: &WebviewWindow) -> tauri::Result<()> {
    if backend() == WindowBackend::WaylandLayerShell {
        let focused = window.clone();
        return window.run_on_main_thread(move || match focused.gtk_window() {
            Ok(native) => {
                native.set_keyboard_mode(KeyboardMode::OnDemand);
                if focused.label() == SurfaceRole::Overlay.label()
                    && let Err(error) = super::focus_grab::activate(&focused)
                {
                    tracing::error!(%error, "failed to activate Wayland outside-click dismissal");
                }
            }
            Err(error) => tracing::error!(%error, "failed to enable Wayland keyboard input"),
        });
    }
    window.set_focus()
}

fn setup_layer_surface(
    window: &gtk::ApplicationWindow,
    level: WindowLevel,
    position: BarPosition,
    monitor: &DisplaySnapshot,
) -> anyhow::Result<()> {
    window.init_layer_shell();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    window.set_namespace(super::layer::namespace(desktop.as_deref()));
    window.set_layer(super::layer::for_level(level));
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_anchor(Edge::Left, false);
    set_vertical_anchor(window, position);
    let native = super::monitor::find(monitor)
        .context("selected Wayland monitor disappeared before bar setup")?;
    window.set_monitor(&native);
    tracing::info!("Wayland layer-shell surface initialized");
    Ok(())
}

fn set_vertical_anchor(window: &gtk::ApplicationWindow, position: BarPosition) {
    let top = position == BarPosition::Top;
    window.set_anchor(Edge::Top, top);
    window.set_anchor(Edge::Bottom, !top);
    window.set_layer_shell_margin(Edge::Top, 0);
    window.set_layer_shell_margin(Edge::Bottom, 0);
}
