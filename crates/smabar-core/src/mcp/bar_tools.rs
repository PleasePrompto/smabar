//! MCP tools for bar state, settings, tile order, and provider data.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::config::update::{self, SetPathError};
use crate::config::{BarPosition, ConfigError};

use super::SmabarMcp;
use super::types::{
    AckResult, BarSetPositionParams, BarStateResult, PluginOrderSetParams, ProviderSnapshotResult,
    SettingsGetParams, SettingsGetResult, SettingsSetParams,
};

#[tool_router(router = bar_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Current bar state: layout, zOrder, language, theme, appearance, pluginOrder, \
                       pluginsHidden, pluginsDeactivated, shortcuts, effects, popups, MCP \
                       settings, settingsWindow dimensions, and all plugins with their status. \
                       The two off-switches are \
                       different things: pluginsHidden holds TILE ids that are hidden while \
                       their plugin keeps running, pluginsDeactivated holds PLUGIN ids with no \
                       process at all."
    )]
    pub(super) async fn bar_get_state(&self) -> Result<Json<BarStateResult>, McpError> {
        let config = self.config.current();
        let plugins = self.plugin_summaries();
        Ok(Json(BarStateResult {
            layout: config.layout,
            z_order: config.z_order,
            language: config.language,
            theme: config.theme,
            plugin_order: config.plugin_order,
            plugins_hidden: config.plugins_hidden,
            plugins_deactivated: config.plugins_deactivated,
            shortcuts: config.shortcuts,
            effects: config.effects,
            appearance: config.appearance,
            popups: config.popups,
            mcp: config.mcp,
            settings_window: config.settings_window,
            plugins,
        }))
    }

    #[tool(
        description = "Move the bar to the top or bottom screen edge (applies live; sets \
                       layout.position). Valid values: top, bottom."
    )]
    pub(super) async fn bar_set_position(
        &self,
        Parameters(BarSetPositionParams { position }): Parameters<BarSetPositionParams>,
    ) -> Result<Json<AckResult>, McpError> {
        self.config
            .update(|current| {
                let mut updated = current.clone();
                updated.layout.position = position;
                (updated, ())
            })
            .map_err(apply_error)?;
        let position_name = match position {
            BarPosition::Top => "top",
            BarPosition::Bottom => "bottom",
        };
        Ok(Json(AckResult {
            message: format!("bar moved to layout.position={position_name}"),
        }))
    }

    #[tool(
        description = "Read the whole config, or the value at a dotted path such as \
                       `layout.position`, `settingsWindow.width`, `mcp.port`, or \
                       `plugins.hello.city`."
    )]
    pub(super) async fn settings_get(
        &self,
        Parameters(SettingsGetParams { path }): Parameters<SettingsGetParams>,
    ) -> Result<Json<SettingsGetResult>, McpError> {
        let root = serde_json::to_value(self.config.current()).map_err(|error| {
            McpError::internal_error(format!("cannot serialize config: {error}"), None)
        })?;
        match path {
            None => Ok(Json(SettingsGetResult {
                path: None,
                value: root,
            })),
            Some(path) => {
                let value = update::get_config_path(&root, &path).ok_or_else(|| {
                    McpError::invalid_params(format!("no value at config path \"{path}\""), None)
                })?;
                Ok(Json(SettingsGetResult {
                    value: value.clone(),
                    path: Some(path),
                }))
            }
        }
    }

    #[tool(
        description = "Set a config value at a dotted path. Allowed roots: zOrder, language, \
                       theme, pluginOrder, plugins, mcp, rendering, layout, shortcuts, \
                       pluginsHidden, pluginsDeactivated, effects, appearance, popups, \
                       settingsWindow, audio (smabar playback volume/mute, notificationSounds, \
                       and per-plugin levels). Intermediate objects are created under `plugins` \
                       and `audio.plugins`. \
                       Everything applies live except mcp.enabled / mcp.port and rendering \
                       (auto|native|software, Linux only; app restart). \
                       Setting `theme` activates the \
                       theme: its tokens apply AND its `settings` block (if any) is applied \
                       one-shot — the user may change anything afterwards; re-activating resets \
                       to the theme again. pluginsHidden and pluginsDeactivated replace the \
                       WHOLE list; plugin_set_visible and plugin_set_active change one entry and \
                       explain what each of them actually does."
    )]
    pub(super) async fn settings_set(
        &self,
        Parameters(SettingsSetParams { path, value }): Parameters<SettingsSetParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let result = self
            .config
            .update(|current| {
                match update::set_config_path_activating(&self.paths, current, &path, value) {
                    Ok((new, theme_warnings)) => (new, Ok(theme_warnings)),
                    Err(error) => (current.clone(), Err(error)),
                }
            })
            .map_err(apply_error)?;
        let theme_warnings = result.map_err(set_path_error)?;
        let restart_note = if matches!(path.split('.').next(), Some("mcp" | "rendering")) {
            " (takes effect after an app restart)"
        } else {
            ""
        };
        let warning_note = if theme_warnings.is_empty() {
            String::new()
        } else {
            format!("; skipped theme setting(s): {}", theme_warnings.join("; "))
        };
        Ok(Json(AckResult {
            message: format!("\"{path}\" updated{restart_note}{warning_note}"),
        }))
    }

    #[tool(
        description = "Set the tile display order (applies live). Ids listed first appear \
                       in that order; unlisted tiles follow in registration order. Every \
                       tile id has the form `plugin:<pluginId>:<tileId>`."
    )]
    pub(super) async fn plugin_order_set(
        &self,
        Parameters(PluginOrderSetParams { order }): Parameters<PluginOrderSetParams>,
    ) -> Result<Json<AckResult>, McpError> {
        self.config
            .update(|current| {
                let mut updated = current.clone();
                updated.plugin_order = order;
                (updated, ())
            })
            .map_err(apply_error)?;
        Ok(Json(AckResult {
            message: "tile order updated".to_string(),
        }))
    }

    #[tool(
        description = "Cached values of system providers subscribed at least once. An empty result \
                       does not mean a provider is unavailable: plugin_guide(section=\"capabilities\") \
                       lists this host's availableProviders even with no plugins installed. \
                       See plugin_guide(section=\"sdk\") for subscription and control APIs."
    )]
    pub(super) async fn provider_snapshot(&self) -> Result<Json<ProviderSnapshotResult>, McpError> {
        Ok(Json(ProviderSnapshotResult {
            providers: self.hub.snapshot(),
        }))
    }
}

pub(super) fn apply_error(error: ConfigError) -> McpError {
    McpError::internal_error(format!("cannot persist config: {error}"), None)
}

/// Path/value mistakes become invalid_params; only serialization of the
/// current config (a smabar bug) is an internal error.
fn set_path_error(error: SetPathError) -> McpError {
    match error {
        SetPathError::Serialize { .. } => McpError::internal_error(error.to_string(), None),
        _ => McpError::invalid_params(error.to_string(), None),
    }
}
