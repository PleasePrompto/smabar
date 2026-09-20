//! Config-facing commands and the config→shell event bridge.

use serde::Serialize;
use serde_json::Value;
use smabar_core::config::{
    AppearanceConfig, BarPosition, EffectsConfig, LayoutBehavior, LayoutConfig, McpConfig,
    PopupsConfig, RenderingMode, SettingsWindowConfig, SmabarPaths, ZOrder,
    update as config_update,
};
use smabar_core::i18n::{self, LocaleMap};
use smabar_core::platform::render::Applied;
use smabar_core::themes::{self, ThemeMap};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast::error::RecvError;

use super::AppState;
use super::shortcuts::ShortcutsUiState;
use crate::platform::strut::{self, DockEdge};
use crate::platform::window as bar_window;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    language: String,
    layout: LayoutConfig,
    z_order: ZOrder,
    /// Tile ids hidden from the bar; their plugins keep running.
    plugins_hidden: Vec<String>,
    /// Plugin ids switched off — no process runs for them.
    plugins_deactivated: Vec<String>,
    effects: EffectsConfig,
    shortcuts: ShortcutsUiState,
    locale: LocaleMap,
    theme: ThemeMap,
    /// Name of the active theme (`config.theme`) — `theme` above is the
    /// resolved token map, which no longer carries the name.
    theme_name: String,
    appearance: AppearanceConfig,
    popups: PopupsConfig,
    settings_window: SettingsWindowConfig,
    plugin_order: Vec<String>,
    /// Per-plugin settings (`config.plugins`), so the settings panel can
    /// render each plugin's `settingsSchema` with its current values.
    plugins: std::collections::BTreeMap<String, Value>,
    /// Root of the per-plugin data directories, so plugin HTML can address
    /// files it wrote via `sb-asset:<relative path>`.
    data_root: String,
    /// Unprivileged loopback page that keeps remote players out of the shell;
    /// native WebView hooks identify its provider document requests.
    embed_root: String,
    /// The bundled terms of use are not accepted; the bar shows only the
    /// legal tile until they are.
    legal_required: bool,
    /// Process-only diagnostic mode; absent in normal operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_probe: Option<&'static str>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LocaleChanged {
    language: String,
    locale: LocaleMap,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LayoutChanged {
    layout: LayoutConfig,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ZOrderChanged {
    z_order: ZOrder,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ThemeChanged {
    theme: ThemeMap,
    theme_name: String,
}

/// Emits `theme-changed` with the freshly resolved token map. The config
/// event bridge fires only when the theme NAME changes; commands that
/// rewrite the ACTIVE theme's file (save-as over it, import overwriting it)
/// call this manually so the shell repaints from the new file.
pub(crate) fn emit_theme_changed(app: &AppHandle, paths: &SmabarPaths, name: &str) {
    let payload = ThemeChanged {
        theme: themes::resolve(paths, name),
        theme_name: name.to_string(),
    };
    let _ = app.emit("theme-changed", payload);
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppearanceChanged {
    appearance: AppearanceConfig,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PopupsChanged {
    popups: PopupsConfig,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SettingsWindowChanged {
    settings_window: SettingsWindowConfig,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PluginOrderChanged {
    order: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EffectsChanged {
    effects: EffectsConfig,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PluginsHiddenChanged {
    disabled: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PluginsDeactivatedChanged {
    deactivated: Vec<String>,
}

/// Initial state for the shell: configured language, layout, shortcuts,
/// effects, resolved locale, and resolved theme tokens.
#[tauri::command]
pub fn get_ui_state(
    state: State<'_, AppState>,
    probe: State<'_, crate::memory_probe::Mode>,
) -> UiState {
    let config = state.watcher.current();
    UiState {
        locale: i18n::resolve(&state.paths, &config.language),
        theme: themes::resolve(&state.paths, &config.theme),
        theme_name: config.theme.clone(),
        appearance: config.appearance,
        popups: config.popups,
        settings_window: config.settings_window,
        language: config.language,
        layout: config.layout,
        z_order: config.z_order,
        plugins_hidden: config.plugins_hidden,
        plugins_deactivated: config.plugins_deactivated,
        effects: config.effects,
        shortcuts: ShortcutsUiState::resolve(&state.shortcuts, &config.shortcuts),
        plugin_order: config.plugin_order,
        plugins: config.plugins,
        data_root: state.paths.data_dir().display().to_string(),
        embed_root: state.embed_url(),
        legal_required: !smabar_core::legal::is_accepted(&state.paths),
        memory_probe: probe.name(),
    }
}

/// Sets a config value at a dotted path (same roots, validation, and theme
/// activation as the MCP `settings_set` tool — both call
/// `config::update::set_config_path_activating`). Skipped theme-settings
/// entries are already logged by the activation; the theme still applies.
#[tauri::command]
pub fn update_config(state: State<'_, AppState>, path: String, value: Value) -> Result<(), String> {
    let result = state
        .watcher
        .update(|current| {
            match config_update::set_config_path_activating(&state.paths, current, &path, value) {
                Ok((new, _theme_warnings)) => (new, Ok(())),
                Err(error) => (current.clone(), Err(error)),
            }
        })
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

/// All known themes with preview colors (compiled-in defaults plus drop-ins)
/// for the settings panel's theme picker.
#[tauri::command]
pub fn list_themes(state: State<'_, AppState>) -> Vec<themes::ThemeInfo> {
    themes::summaries(&state.paths, &state.watcher.current())
}

/// What the System settings need and `get_ui_state` does not carry: the
/// installed locales (bundled plus drop-ins from `~/.smabar/locales/`), the
/// MCP endpoint's own config, and the rendering decision. Read on demand
/// rather than pushed into the shell store. The bar also reads the update
/// channel before scheduling application update checks.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    update_channel: UpdateChannel,
    languages: Vec<String>,
    mcp: McpConfig,
    /// `None` off Linux, where no renderer choice is needed.
    rendering: Option<RenderingStatus>,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[cfg(not(feature = "no-self-update"))]
    App,
    #[cfg(feature = "no-self-update")]
    Store,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenderingStatus {
    mode: RenderingMode,
    startup_mode: RenderingMode,
    applied: Applied,
}

#[tauri::command]
pub fn get_system_settings(state: State<'_, AppState>) -> SystemSettings {
    let config = state.watcher.current();
    SystemSettings {
        #[cfg(not(feature = "no-self-update"))]
        update_channel: UpdateChannel::App,
        #[cfg(feature = "no-self-update")]
        update_channel: UpdateChannel::Store,
        languages: i18n::available_languages(&state.paths),
        mcp: config.mcp,
        rendering: state.rendering().map(|plan| RenderingStatus {
            mode: config.rendering,
            startup_mode: plan.requested,
            applied: plan.applied,
        }),
    }
}

/// Forwards config-file changes to the shell (`locale-changed`,
/// `layout-changed`, `z-order-changed`, `theme-changed`, `plugin-order-changed`,
/// `shortcuts-changed`, `effects-changed`, `appearance-changed`,
/// `popups-changed`, `plugins-hidden-changed`,
/// `plugins-deactivated-changed`, `settings-window-changed`).
pub fn spawn_config_events(
    app: AppHandle,
    watcher: &smabar_core::config::ConfigWatcher,
    paths: SmabarPaths,
    shortcuts: smabar_core::shortcuts::ShortcutsService,
) {
    let mut rx = watcher.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(change) => {
                    let position_changed = change.old.layout.position != change.new.layout.position;
                    let monitor_changed = change.old.layout.monitor != change.new.layout.monitor;
                    let monitor_relocated = if monitor_changed {
                        match app
                            .state::<crate::surfaces::SurfaceManager>()
                            .reconcile_monitors(&app, None, "preference")
                            .await
                        {
                            Ok(relocated) => relocated,
                            Err(error) => {
                                tracing::error!(%error, "failed to apply monitor preference; keeping the current monitor");
                                false
                            }
                        }
                    } else {
                        false
                    };
                    if position_changed && !monitor_relocated {
                        if let Err(error) = app
                            .state::<crate::surfaces::SurfaceManager>()
                            .prepare_bar_relocation(&app, change.new.layout.position)
                            .await
                        {
                            tracing::warn!(%error, "failed to clear bar overlays before moving edge");
                        }
                        if let Some(window) = app.get_webview_window("bar")
                            && let Err(error) = strut::apply(&window, None, None)
                        {
                            tracing::warn!(%error, "failed to release the old bar-edge reservation");
                        }
                    }
                    if change.language_changed() {
                        let payload = LocaleChanged {
                            locale: i18n::resolve(&paths, &change.new.language),
                            language: change.new.language.clone(),
                        };
                        let title = crate::surfaces::settings_window_title(&payload.locale);
                        let _ = app.emit("locale-changed", payload);
                        if let Some(window) =
                            app.get_webview_window(crate::surfaces::SurfaceRole::Settings.label())
                            && let Err(error) = window.set_title(&title)
                        {
                            tracing::warn!(%error, "failed to retitle the settings window");
                        }
                    }
                    if change.theme_changed() {
                        emit_theme_changed(&app, &paths, &change.new.theme);
                    }
                    if change.layout_changed() {
                        let _ = app.emit(
                            "layout-changed",
                            LayoutChanged {
                                layout: change.new.layout.clone(),
                            },
                        );
                    }
                    if change.old.z_order != change.new.z_order {
                        let _ = app.emit(
                            "z-order-changed",
                            ZOrderChanged {
                                z_order: change.new.z_order,
                            },
                        );
                    }
                    if change.shortcuts_changed() {
                        // No payload: resolved shortcuts carry inline icon
                        // data URIs, far too heavy to broadcast — the shell
                        // refetches via `get_shortcuts`.
                        let _ = app.emit("shortcuts-changed", ());
                        // A pin added just now has no cached icon yet.
                        crate::icons::spawn_fetch(app.clone(), shortcuts.clone(), &change.new);
                    }
                    if change.effects_changed() {
                        let _ = app.emit(
                            "effects-changed",
                            EffectsChanged {
                                effects: change.new.effects.clone(),
                            },
                        );
                    }
                    if change.old.appearance != change.new.appearance {
                        let _ = app.emit(
                            "appearance-changed",
                            AppearanceChanged {
                                appearance: change.new.appearance.clone(),
                            },
                        );
                    }
                    if change.old.audio != change.new.audio {
                        let _ = app.emit("audio-settings-changed", &change.new.audio);
                    }
                    if change.old.popups != change.new.popups {
                        let _ = app.emit(
                            "popups-changed",
                            PopupsChanged {
                                popups: change.new.popups.clone(),
                            },
                        );
                    }
                    if change.settings_window_changed() {
                        let _ = app.emit(
                            "settings-window-changed",
                            SettingsWindowChanged {
                                settings_window: change.new.settings_window,
                            },
                        );
                    }
                    if change.plugins_hidden_changed() {
                        let _ = app.emit(
                            "plugins-hidden-changed",
                            PluginsHiddenChanged {
                                disabled: change.new.plugins_hidden.clone(),
                            },
                        );
                    }
                    // Only the settings list needs this — the tiles of a
                    // deactivated plugin leave the bar through the
                    // supervisor's own `plugin-removed` event.
                    if change.plugins_deactivated_changed() {
                        let _ = app.emit(
                            "plugins-deactivated-changed",
                            PluginsDeactivatedChanged {
                                deactivated: change.new.plugins_deactivated.clone(),
                            },
                        );
                    }
                    if change.old.plugin_order != change.new.plugin_order {
                        let _ = app.emit(
                            "plugin-order-changed",
                            PluginOrderChanged {
                                order: change.new.plugin_order.clone(),
                            },
                        );
                    }
                    let behavior_changed = change.old.layout.behavior != change.new.layout.behavior;
                    let fullscreen_policy_changed = change.old.layout.yield_to_fullscreen
                        != change.new.layout.yield_to_fullscreen;
                    if behavior_changed {
                        app.state::<AppState>().set_fullscreen_active(false);
                    }
                    if behavior_changed && let Some(window) = app.get_webview_window("bar") {
                        let edge = match change.new.layout.position {
                            BarPosition::Top => DockEdge::Top,
                            BarPosition::Bottom => DockEdge::Bottom,
                        };
                        let dock = if change.new.layout.behavior == LayoutBehavior::Reserve {
                            app.state::<crate::surfaces::SurfaceManager>()
                                .bar_geometry(change.new.layout.position)
                                .map(|rect| rect.map(|rect| (edge, rect)))
                        } else {
                            Ok(None)
                        };
                        match dock.and_then(|dock| strut::apply(&window, dock, None)) {
                            Ok(()) => {}
                            Err(error) => tracing::warn!(
                                %error,
                                "failed to apply bar reservation after behavior change"
                            ),
                        }
                    }
                    if (change.old.z_order != change.new.z_order
                        || behavior_changed
                        || fullscreen_policy_changed)
                        && let Some(window) = app.get_webview_window("bar")
                        && let Err(error) = bar_window::apply_window_level(
                            &window,
                            app.state::<AppState>().window_level(),
                        )
                    {
                        tracing::warn!(%error, "failed to apply native window-level change");
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "config event listener lagged");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}
