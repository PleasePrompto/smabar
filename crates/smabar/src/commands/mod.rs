//! Tauri commands and core→shell events, split by concern. Plugin forwarding
//! lives in `plugin_events`.

// Public modules: `generate_handler!` needs the full path to a command (the
// macro resolves hidden `__cmd__*` items next to the function, which plain
// `pub use` re-exports cannot carry).
pub mod bar_geometry;
pub mod config;
pub mod legal;
mod plugin_events;
pub mod runtime;
pub mod shortcuts;
pub mod store;
pub mod themes;
#[cfg_attr(feature = "no-self-update", path = "update_store.rs")]
pub mod update;

pub use config::spawn_config_events;
pub use plugin_events::spawn_plugin_events;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use smabar_core::config::{
    ConfigError, ConfigWatcher, LayoutBehavior, SettingsWindowConfig, SmabarConfig, SmabarPaths,
    ZOrder,
};
use smabar_core::platform::WindowLevel;
use smabar_core::platform::render::RenderPlan;
use smabar_core::plugins::PluginSupervisor;
use smabar_core::shortcuts::ShortcutsService;
use smabar_core::store::StoreService;
use tauri::State;

pub struct AppState {
    paths: SmabarPaths,
    watcher: Arc<ConfigWatcher>,
    supervisor: PluginSupervisor,
    pub(crate) plugin_delivery: crate::plugin_delivery::PluginDelivery,
    shortcuts: ShortcutsService,
    store: StoreService,
    embed_server: Option<smabar_core::embed::EmbedServer>,
    /// Rendering decision made before GTK started; absent off Linux.
    rendering: Option<RenderPlan>,
    fullscreen_active: AtomicBool,
    /// Set while `install_update` runs; a second call is refused.
    #[cfg(not(feature = "no-self-update"))]
    update_in_progress: AtomicBool,
}

impl AppState {
    pub fn new(
        paths: SmabarPaths,
        watcher: Arc<ConfigWatcher>,
        supervisor: PluginSupervisor,
        shortcuts: ShortcutsService,
        store: StoreService,
        embed_server: Option<smabar_core::embed::EmbedServer>,
        rendering: Option<RenderPlan>,
    ) -> Self {
        Self {
            paths,
            watcher,
            supervisor,
            plugin_delivery: crate::plugin_delivery::PluginDelivery::default(),
            shortcuts,
            store,
            embed_server,
            rendering,
            fullscreen_active: AtomicBool::new(false),
            #[cfg(not(feature = "no-self-update"))]
            update_in_progress: AtomicBool::new(false),
        }
    }

    /// Current native level for the persistent bar only.
    /// Remembers the settings window's geometry the way `update_config`
    /// writes: one atomic config write, echoed to every subscriber.
    pub fn remember_settings_window(
        &self,
        geometry: SettingsWindowConfig,
    ) -> Result<(), ConfigError> {
        self.watcher.update(|current| {
            let mut next = current.clone();
            next.settings_window = geometry;
            (next, ())
        })
    }

    pub fn window_level(&self) -> WindowLevel {
        effective_window_level(
            &self.watcher.current(),
            self.fullscreen_active.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn set_fullscreen_active(&self, active: bool) -> bool {
        let active = active && self.watcher.current().layout.behavior == LayoutBehavior::Reserve;
        self.fullscreen_active.swap(active, Ordering::Relaxed) != active
    }

    pub(crate) fn store(&self) -> &StoreService {
        &self.store
    }

    pub(crate) fn paths(&self) -> &SmabarPaths {
        &self.paths
    }

    pub(crate) fn config(&self) -> SmabarConfig {
        self.watcher.current()
    }

    pub(crate) fn rendering(&self) -> Option<&RenderPlan> {
        self.rendering.as_ref()
    }

    pub(crate) fn embed_url(&self) -> String {
        self.embed_server
            .as_ref()
            .map(smabar_core::embed::EmbedServer::url)
            .unwrap_or_default()
    }

    /// Tauri's event loop exits the process directly, so plugin child
    /// processes must receive their graceful shutdown before `App::run` ends.
    pub(crate) async fn shutdown_plugins(&self) {
        self.supervisor.shutdown_all().await;
    }
}

/// Autohide must stay reachable above normal windows, regardless of the
/// independently persisted stacking preference.
fn effective_window_level(config: &SmabarConfig, fullscreen_active: bool) -> WindowLevel {
    if fullscreen_active
        && config.layout.yield_to_fullscreen
        && config.layout.behavior == LayoutBehavior::Reserve
    {
        return WindowLevel::Bottom;
    }
    let z_order = match config.layout.behavior {
        LayoutBehavior::Autohide => ZOrder::Top,
        LayoutBehavior::Reserve | LayoutBehavior::Float => config.z_order,
    };
    match z_order {
        ZOrder::Bottom => WindowLevel::Bottom,
        ZOrder::Top if !config.layout.yield_to_fullscreen => WindowLevel::Top,
        ZOrder::Top if config.layout.behavior == LayoutBehavior::Reserve => WindowLevel::Panel,
        ZOrder::Top => WindowLevel::Top,
    }
}

/// Gives the bar window keyboard focus. The bar is a DOCK-type window
/// (required for the strut reservation), and window managers never focus
/// docks on click — so typing into a flyout form would land in whatever
/// window held focus before. The shell calls this when the user focuses an
/// editable element. The Wayland helper enables OnDemand keyboard input,
/// including when a hover preview is promoted to an interactive flyout.
#[tauri::command]
pub fn focus_bar(window: tauri::WebviewWindow) -> Result<(), String> {
    crate::platform::window::focus(&window).map_err(|error| error.to_string())
}

/// The shell's way into the central log.
///
/// Without this the webview had NO logging path at all: every failure went to
/// `globalThis.reportError`, which reaches the browser console and nothing
/// else — not the log file, not MCP, not the user. `console.*` is banned in
/// shell code precisely because it is not a log; this is the sanctioned
/// replacement.
///
/// `pluginId` routes the entry: with it the message lands in that plugin's
/// own log (source `"shell"`), which is where the author's agent already
/// looks when its markup does not show up. Without it, it is a smabar
/// problem and goes to the core log.
#[tauri::command]
pub fn ui_log(
    state: State<'_, AppState>,
    level: String,
    message: String,
    fields: Option<Value>,
    plugin_id: Option<String>,
) {
    if let Some(plugin_id) = plugin_id {
        smabar_core::plugins::append_plugin_log(
            &state.paths.logs_dir(),
            &plugin_id,
            &level,
            "shell",
            &message,
            fields.as_ref(),
        );
        return;
    }
    log_shell_event(&level, &message, fields.unwrap_or(Value::Null));
}

/// Emits one shell event into the core log under target `smabar::shell`.
///
/// tracing levels are compile-time, so the runtime string is matched here.
/// An unknown level is a shell bug, not a reason to drop the message — it
/// falls back to info rather than vanishing.
fn log_shell_event(level: &str, message: &str, fields: Value) {
    match level {
        "error" => tracing::error!(target: "smabar::shell", %fields, "{message}"),
        "warn" => tracing::warn!(target: "smabar::shell", %fields, "{message}"),
        "debug" => tracing::debug!(target: "smabar::shell", %fields, "{message}"),
        _ => tracing::info!(target: "smabar::shell", %fields, "{message}"),
    }
}

/// Opens a plugin-provided http(s) link in the system browser. The shell
/// intercepts every anchor click inside plugin shadow roots (the webview
/// itself must never navigate) and forwards the href here; the core
/// validates the URL and forwards it to the native platform opener.
#[tauri::command(async)]
pub fn open_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    state
        .shortcuts
        .open_url(&url)
        .map_err(|error| error.to_string())
}

/// Currently registered plugins, shaped like `plugin-added` payloads. The
/// shell calls this after attaching its listeners — the original `Added`
/// events fired during startup, before the webview existed.
#[tauri::command]
pub fn get_plugins(state: State<'_, AppState>) -> Vec<Value> {
    state
        .supervisor
        .current_plugins()
        .into_iter()
        .map(|manifest| {
            json!({
                "pluginId": manifest.id,
                "name": manifest.name,
                "iconDataUrl": manifest.icon_data_url,
                "tiles": manifest.tiles,
                "settingsSchema": manifest.settings_schema,
            })
        })
        .collect()
}

/// Last rendered HTML needed by the calling surface, shaped like live UI
/// payloads. The shell calls this after attaching its listeners — renders
/// pushed before the webview existed (startup, dev reload) are replayed here
/// so slow-polling plugins don't leave empty tiles.
#[tauri::command]
pub fn get_plugin_ui(window: tauri::WebviewWindow, state: State<'_, AppState>) -> Vec<Value> {
    if window.label() == crate::surfaces::SurfaceRole::Bar.label() {
        state.plugin_delivery.bar_snapshot()
    } else {
        Vec::new()
    }
}

/// Every INSTALLED plugin with its lifecycle status — including the ones that
/// are deactivated or failed, which `get_plugins` cannot show because they
/// contribute no tiles. The settings panel needs exactly that list: a plugin
/// the user switched off has to stay visible, or there is no way to switch it
/// back on.
#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> Vec<Value> {
    let summaries = state.store.installed_summaries();
    state
        .supervisor
        .plugin_infos()
        .into_iter()
        .map(|info| {
            let summary = summaries.get(&info.id);
            let manifest = info.manifest.as_ref();
            json!({
                "id": info.id,
                "name": manifest.map(|manifest| &manifest.name),
                "description": manifest.and_then(|manifest| manifest.description.as_ref()),
                "iconDataUrl": manifest.and_then(|manifest| manifest.icon_data_url.as_ref()),
                "settingsSchema": manifest.and_then(|manifest| manifest.settings_schema.as_ref()),
                "tiles": manifest.map_or(&[][..], |manifest| manifest.tiles.as_slice()),
                "status": info.status,
                "error": info.error,
                "origin": summary.map_or(smabar_core::store::PluginOrigin::User, |summary| summary.origin),
                "version": summary.and_then(|summary| summary.version.clone()),
                "update": summary.and_then(|summary| summary.update.clone()),
                "modified": summary.is_some_and(|summary| summary.modified),
                "blocked": summary.and_then(|summary| summary.blocked.clone()),
            })
        })
        .collect()
}

/// Permanently deletes a plugin (code, data, log) through the same core
/// function the MCP `plugin_remove` tool calls. Irreversible — the settings
/// panel confirms before it gets here.
#[tauri::command]
pub async fn remove_plugin(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .supervisor
        .remove(&plugin_id)
        .await
        .map_err(|error| error.to_string())?;
    state.store.note_removed();
    Ok(())
}

/// Forwards a UI interaction (`data-action`) to the plugin.
#[tauri::command]
pub async fn plugin_action(
    state: State<'_, AppState>,
    desktop: State<'_, crate::desktop::DesktopServices>,
    plugin_id: String,
    tile_id: String,
    action: String,
    value: Option<Value>,
    popup_instance_id: Option<u64>,
) -> Result<(), String> {
    if let Some(instance) = popup_instance_id {
        return desktop
            .action(instance, &plugin_id, &tile_id, &action, value)
            .await;
    }
    state
        .supervisor
        .dispatch_action(&plugin_id, &tile_id, &action, value)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
