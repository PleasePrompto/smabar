#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod desktop;
mod desktop_types;
mod fonts;
mod frame_ipc;
mod http_util;
mod icons;
mod memory_probe;
mod platform;
mod plugin_delivery;
mod provision;
mod runtime_paths;
mod store_http;
mod surfaces;
mod tray;

use std::sync::Arc;

use anyhow::Context;
use smabar_core::config::{ConfigWatcher, RenderingMode, SmabarConfig, SmabarPaths};
use smabar_core::plugins::{
    PluginSupervisor, SupervisorOptions, seed_bundled_plugins, sweep_orphaned_data,
};
use smabar_core::providers::ProviderHub;
use smabar_core::shortcuts::ShortcutsService;
use tauri::Manager;

fn main() -> anyhow::Result<()> {
    let paths = SmabarPaths::default_base().context("could not determine home directory")?;
    let provisioning = std::env::args().any(|argument| argument == "--provision");

    // WebKit reads these variables while starting its web process, before
    // GTK and the logging thread exist. The watcher loads the config again
    // in setup; an invalid file falls back to auto here and fails there.
    let startup_config = (!provisioning).then(|| SmabarConfig::load(&paths));
    let rendering_mode = startup_config
        .as_ref()
        .and_then(|loaded| loaded.as_ref().ok())
        .map_or_else(RenderingMode::default, |config| config.rendering);
    let rendering = (!provisioning)
        .then(|| platform::prepare_rendering(rendering_mode))
        .flatten();

    let _log_guard = smabar_core::logging::init(&paths).context("failed to initialise logging")?;
    let context = tauri::generate_context!();
    let uv_override = runtime_paths::resolve_uv(context.package_info());

    // Headless installer mode: provision the Python runtime and exit. Runs
    // BEFORE the builder on purpose — no window, and the single-instance
    // plugin is never registered, so a running bar is not disturbed and
    // cannot swallow this launch.
    if provisioning {
        let code = provision::run(&paths, uv_override);
        drop(_log_guard);
        std::process::exit(code);
    }

    let probe_value = std::env::var_os("SMABAR_MEMORY_PROBE");
    let memory_probe = probe_value
        .as_ref()
        .map(|value| value.to_str().context("SMABAR_MEMORY_PROBE must contain valid Unicode"))
        .transpose()
        .and_then(memory_probe::Mode::parse)
        .inspect_err(|error| tracing::error!(%error, "invalid memory probe; unset SMABAR_MEMORY_PROBE and restart"))?;
    if let Some(mode) = memory_probe.name() {
        tracing::warn!(
            mode,
            "memory probe enabled for this process; unset SMABAR_MEMORY_PROBE and restart for normal operation"
        );
    }

    if let Some(Err(error)) = &startup_config {
        tracing::warn!(
            %error,
            "config.json could not be read before startup; rendering fell back to auto"
        );
    }
    if let Some(plan) = &rendering {
        tracing::info!(
            mode = ?rendering_mode,
            applied = ?plan.applied,
            nvidia = plan.nvidia_detected,
            env = ?plan.env,
            "rendering prepared"
        );
        if let Some(note) = &plan.note {
            tracing::warn!(%note, "rendering deviates from the requested mode");
        }
    }

    platform::check_webview(
        &paths,
        startup_config
            .as_ref()
            .and_then(|config| config.as_ref().ok())
            .map_or("en", |config| config.language.as_str()),
        context
            .config()
            .bundle
            .windows
            .minimum_webview2_version
            .as_deref(),
    )?;
    let builder = frame_ipc::isolate_subframes(tauri::Builder::default())
        // Must be the first plugin: a second launch focuses the existing bar
        // and exits, so stale duplicate windows cannot exist.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("bar") {
                let _ = window.set_focus();
            }
        }));
    #[cfg(not(feature = "no-self-update"))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    let app = builder
        .manage(memory_probe)
        // DnD observability: XDND onto an input-shaped dock window is
        // unproven terrain — these logs show whether drags reach the webview
        // at all (the shell's onDragDropEvent handler does the pinning).
        // `Over` fires per mouse move and stays unlogged on purpose.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(drag) = event
                && !matches!(drag, tauri::DragDropEvent::Over { .. })
            {
                tracing::debug!(surface = window.label(), event = ?drag, "drag-drop window event");
            }
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .app_handle()
                    .state::<surfaces::SurfaceManager>()
                    .window_destroyed(window.app_handle(), window.label());
            }
            if window.label() == surfaces::SurfaceRole::Settings.label()
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                // Alt+F4 and the window manager's close hide the prewarmed
                // window; destroying it would rebuild the whole webview on
                // the next open.
                api.prevent_close();
                let manager = window.app_handle().state::<surfaces::SurfaceManager>();
                if let Err(error) =
                    manager.close(window.app_handle(), surfaces::SurfaceRole::Settings)
                {
                    tracing::error!(%error, "failed to hide the settings window on close request");
                }
            }
            if window.label() == surfaces::SurfaceRole::Overlay.label()
                && let tauri::WindowEvent::Focused(focused) = event
            {
                let manager = window.app_handle().state::<surfaces::SurfaceManager>();
                let result = if *focused {
                    manager.handle_focus_gained()
                } else {
                    manager.handle_focus_lost(window.app_handle())
                };
                if let Err(error) = result {
                    tracing::error!(
                        %error,
                        "failed to dismiss the overlay after focus moved away; press Escape to close it"
                    );
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            platform::input_shape::set_input_shape,
            commands::config::get_ui_state,
            commands::plugin_action,
            commands::get_plugins,
            commands::list_plugins,
            commands::remove_plugin,
            commands::get_plugin_ui,
            commands::bar_geometry::set_bar_geometry,
            surfaces::bar::set_bar_revealed,
            commands::config::update_config,
            commands::config::list_themes,
            commands::config::get_system_settings,
            platform::autostart::get_autostart_status,
            platform::autostart::set_autostart,
            platform::reservation_access::get_reservation_status,
            platform::reservation_access::request_reservation_access,
            commands::themes::save_custom_theme,
            commands::themes::delete_theme,
            commands::themes::export_theme,
            commands::themes::import_theme,
            commands::themes::get_theme_export_dir,
            fonts::font_list,
            fonts::ensure_google_font,
            commands::shortcuts::list_apps,
            commands::shortcuts::get_app_icon,
            commands::shortcuts::get_shortcuts,
            commands::shortcuts::pin_shortcut,
            commands::shortcuts::pin_special_shortcut,
            commands::shortcuts::unpin_shortcut,
            commands::shortcuts::launch_shortcut,
            surfaces::get_surface_context,
            surfaces::monitor::get_monitor_state,
            surfaces::surface_ready,
            surfaces::open_settings,
            surfaces::toggle_settings,
            surfaces::close_settings,
            surfaces::close_surface,
            surfaces::notifications::show_notice,
            surfaces::notifications::set_notification_measure,
            surfaces::notifications::stage_notification_update,
            surfaces::overlay::open_flyout,
            surfaces::overlay::measure_flyout,
            surfaces::overlay::pin_flyout,
            surfaces::overlay::close_flyout,
            surfaces::overlay::set_overlay_pointer,
            surfaces::presentation::finalize_overlay_clear,
            surfaces::menu::open_context_menu,
            surfaces::menu::measure_context_menu,
            surfaces::menu::close_context_menu,
            surfaces::tooltip::open_tooltip,
            surfaces::tooltip::measure_tooltip,
            surfaces::tooltip::close_tooltip,
            commands::focus_bar,
            commands::open_url,
            commands::ui_log,
            desktop::get_managed_popups,
            desktop::get_audio_settings,
            desktop::popup_event_report,
            commands::runtime::get_runtime_status,
            commands::runtime::retry_provisioning,
            commands::update::check_update,
            commands::update::install_update,
            commands::store::store_overview,
            commands::store::store_refresh,
            commands::store::store_detail,
            commands::store::store_install_plugin,
            commands::store::store_install_theme,
            commands::legal::legal_status,
            commands::legal::legal_accept,
            commands::legal::legal_decline,
            capture::bar_reply
        ])
        .setup(move |app| {
            // The app edge is the one place that reads the environment.
            // SMABAR_SDK_PATH (set by dev.sh) wins; installed builds resolve
            // the SDK bundled as a resource (bundle.resources → sdk/python).
            let sdk_path = std::env::var_os("SMABAR_SDK_PATH")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    app.path()
                        .resolve("sdk/python", tauri::path::BaseDirectory::Resource)
                        .ok()
                        .filter(|path| path.is_dir())
                });
            let mut reserved_ids = std::collections::BTreeSet::new();
            match app
                .path()
                .resolve("plugins", tauri::path::BaseDirectory::Resource)
            {
                Ok(bundled_plugins) => {
                    reserved_ids = bundled_plugin_ids(&bundled_plugins);
                    if let Err(error) = seed_bundled_plugins(&paths, &bundled_plugins) {
                        tracing::error!(
                            %error,
                            path = %bundled_plugins.display(),
                            "failed to seed bundled plugins; user-installed plugins will still load"
                        );
                    }
                }
                Err(error) => tracing::warn!(
                    %error,
                    "could not resolve bundled plugin resources; base plugins were not seeded"
                ),
            }
            // An install interrupted mid-swap leaves a plugin folder missing;
            // repair it BEFORE the sweep below mistakes its data for a leftover.
            match smabar_core::store::recover(&paths) {
                Ok(Some(plugin_id)) => tracing::info!(plugin_id, "replayed the store's install journal"),
                Ok(None) => {},
                Err(error) => tracing::error!(%error, "store recovery is pending; plugin data, backups and journal are retained; resolve the error and restart smabar"),
            }
            // After seeding: a plugin installed a moment ago must not look
            // like a leftover.
            sweep_orphaned_data(&paths);
            let (host, host_requests) = smabar_core::plugins::HostPort::channel();
            let options = SupervisorOptions {
                host: Some(host),
                sdk_path,
                uv_override,
                python_install_dir: Some(paths.tools_dir()),
            };
            let hub = ProviderHub::new();
            // ConfigWatcher::spawn and the supervisor need a tokio runtime context.
            let (watcher, supervisor) = tauri::async_runtime::block_on(async {
                let watcher = Arc::new(ConfigWatcher::spawn(paths.clone())?);
                let supervisor = PluginSupervisor::start(
                    paths.clone(),
                    hub.clone(),
                    Arc::clone(&watcher),
                    options,
                )
                .await;
                Ok::<_, smabar_core::config::ConfigError>((watcher, supervisor))
            })?;
            let store = build_store(&paths, &watcher, &supervisor, reserved_ids)?;
            platform::autostart::setup(app.handle(), Arc::clone(&watcher));
            let shortcuts = build_shortcuts_service(&paths, &watcher.current().language)?;
            let locale = smabar_core::i18n::resolve(&paths, &watcher.current().language);
            let embed_server = match tauri::async_runtime::block_on(smabar_core::embed::serve()) {
                Ok(server) => Some(server),
                Err(error) => {
                    tracing::error!(
                        %error,
                        "remote media players are unavailable; local img, audio and video remain supported"
                    );
                    None
                }
            };
            let embed_origin = embed_server
                .as_ref()
                .map(smabar_core::embed::EmbedServer::origin);
            let (initial_ui, plugin_events) = supervisor.subscribe_with_ui();
            app.manage(commands::AppState::new(
                paths.clone(),
                Arc::clone(&watcher),
                supervisor.clone(),
                shortcuts.clone(),
                store.clone(),
                embed_server,
                rendering,
            ));
            app.state::<commands::AppState>().plugin_delivery.reset(initial_ui);
            app.manage(surfaces::SurfaceManager::new(
                embed_origin,
                surfaces::settings_window_title(&locale),
            ));
            let surface_manager = app.state::<surfaces::SurfaceManager>();
            let bar = surface_manager.create_bar(app.handle())?;
            tauri::async_runtime::block_on(async {
                desktop::DesktopServices::start(app.handle(), host_requests, watcher.clone())
            })?;
            platform::install_monitor_watch(app.handle())?;
            surfaces::install_pointer_watchdog(app.handle());
            icons::spawn_fetch(app.handle().clone(), shortcuts.clone(), &watcher.current());
            commands::spawn_config_events(
                app.handle().clone(),
                &watcher,
                paths.clone(),
                shortcuts.clone(),
            );
            commands::spawn_plugin_events(app.handle().clone(), &supervisor, plugin_events);
            commands::runtime::spawn_runtime_events(app.handle().clone(), &supervisor);
            commands::store::spawn_store_events(app.handle().clone(), &store);
            commands::store::spawn_store_refresh_timer(store.clone());
            let bar_port = capture::install(app.handle());
            spawn_mcp_server(
                &paths,
                &watcher,
                &hub,
                &supervisor,
                shortcuts.clone(),
                &store,
                bar_port,
            );
            if let Err(error) = tray::setup(app, &locale) {
                tracing::warn!(%error, "failed to create the tray icon; smabar will keep running");
            }
            platform::window::apply_window_level(
                &bar,
                app.state::<commands::AppState>().window_level(),
            )?;
            surface_manager.prewarm_overlay(app.handle())?;
            if !smabar_core::legal::is_accepted(&paths) {
                // Presented by `mark_ready` once the settings shell reports in.
                if let Err(error) = surfaces::open_settings_from_app(app.handle(), "legal") {
                    tracing::warn!(%error, "could not open the legal notice at startup; the bar offers it as its only tile");
                }
            }
            Ok(())
        })
        .build(context)
        .context("failed to build smabar")?;
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            surfaces::remember_settings_geometry_at_exit(app);
            tauri::async_runtime::block_on(
                app.state::<commands::AppState>().shutdown_plugins(),
            );
            if let Err(error) = platform::shutdown() {
                tracing::error!(%error, "failed to release native platform state before exit; restart the desktop session if its reserved screen edge remains");
            }
        }
    });
    Ok(())
}

/// The ids of the plugins that ship with smabar: the folder names of the
/// bundled resources. The store never installs over them.
fn bundled_plugin_ids(bundled_dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    std::fs::read_dir(bundled_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The Community Store client over the app's HTTP edge. The endpoint and
/// the key are the app edge's decisions (environment overrides live here).
fn build_store(
    paths: &SmabarPaths,
    watcher: &Arc<ConfigWatcher>,
    supervisor: &PluginSupervisor,
    reserved_ids: std::collections::BTreeSet<String>,
) -> anyhow::Result<smabar_core::store::StoreService> {
    let endpoint = store_http::store_endpoint().map_err(anyhow::Error::msg)?;
    let key = store_http::store_public_key().map_err(anyhow::Error::msg)?;
    let fetcher = Arc::new(store_http::ReqwestFetcher::new(&endpoint)?);
    smabar_core::store::StoreService::new(
        paths.clone(),
        Arc::clone(watcher),
        supervisor.clone(),
        fetcher,
        smabar_core::store::StoreOptions {
            endpoint,
            key,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            reserved_ids,
        },
    )
    .context("cannot start the Community Store client")
}

/// Injects the current platform's shortcut behavior. The home directory is
/// the parent of `~/.smabar`, from the same `SmabarPaths` source of truth.
fn build_shortcuts_service(
    paths: &SmabarPaths,
    language: &str,
) -> anyhow::Result<ShortcutsService> {
    let home = paths
        .base_dir()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let platform = platform::shortcut_platform(&home, &paths.icons_dir(), language)?;
    Ok(ShortcutsService::new(platform, paths.icons_dir()))
}

/// Starts the embedded MCP server when `mcp.enabled` (default). A failed
/// bind is logged but never crashes the bar; the server then lives for the
/// process lifetime (dropping the handle detaches it).
fn spawn_mcp_server(
    paths: &SmabarPaths,
    watcher: &Arc<ConfigWatcher>,
    hub: &ProviderHub,
    supervisor: &PluginSupervisor,
    shortcuts: ShortcutsService,
    store: &smabar_core::store::StoreService,
    bar: smabar_core::capture::BarPort,
) {
    let mcp_config = watcher.current().mcp;
    if !mcp_config.enabled {
        tracing::info!("MCP server disabled via mcp.enabled in config.json");
        return;
    }
    let handler = smabar_core::mcp::SmabarMcp::new(
        paths.clone(),
        Arc::clone(watcher),
        hub.clone(),
        supervisor.clone(),
        shortcuts,
        bar,
    )
    .with_store(store.clone());
    tauri::async_runtime::spawn(async move {
        match smabar_core::mcp::serve(handler, mcp_config.port).await {
            Ok(server) => {
                let addr = server.addr();
                tracing::info!(%addr, "MCP server listening at http://{addr}/mcp");
            }
            Err(error) => tracing::error!(%error, "failed to start the MCP server"),
        }
    });
}
