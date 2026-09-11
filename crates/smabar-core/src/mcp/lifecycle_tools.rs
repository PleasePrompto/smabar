//! MCP tools for the three ways to switch something off: hide a tile,
//! deactivate a plugin, delete a plugin.
//!
//! They are three different things and the tool descriptions have to say so —
//! an agent that reads only the names would otherwise pick whichever it saw
//! first. All three write through the shared `smabar-core` logic, so they
//! behave exactly like the same action taken in the settings panel.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::plugins::{self, RemoveError};

use super::SmabarMcp;
use super::bar_tools::apply_error;
use super::plugin_tools::validate_plugin_id;
use super::plugin_types::PluginIdParams;
use super::types::{AckResult, PluginSetActiveParams, PluginSetVisibleParams};

#[tool_router(router = lifecycle_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Stop or start a plugin's PROCESS (persisted in `pluginsDeactivated`). \
                       active=false: no process, no polling, no popups, stays off across \
                       restarts and plugin_reload refuses it; folder, data directory and \
                       settings are kept, so active=true brings it back exactly as it was. Its \
                       tiles leave the bar meanwhile. Hide one tile while the plugin keeps \
                       running: plugin_set_visible. Delete for good: plugin_remove."
    )]
    pub(super) async fn plugin_set_active(
        &self,
        Parameters(PluginSetActiveParams { id, active }): Parameters<PluginSetActiveParams>,
    ) -> Result<Json<AckResult>, McpError> {
        validate_plugin_id(&id)?;
        // Deactivating an id nothing is installed under would write a config
        // entry that silently kills a future plugin of that name.
        if self.supervisor.plugin_dir(&id).is_none() {
            return Err(McpError::invalid_params(
                format!("no plugin \"{id}\" is installed; plugin_list shows the ids that exist"),
                None,
            ));
        }
        let changed = plugins::set_plugin_active(&self.config, &id, active).map_err(apply_error)?;
        let state = if active { "active" } else { "deactivated" };
        let message = if changed {
            format!(
                "plugin \"{id}\" is now {state}{}",
                if active {
                    "; its process is starting"
                } else {
                    "; its process was stopped and its tiles left the bar"
                }
            )
        } else {
            format!("plugin \"{id}\" was already {state}; nothing changed")
        };
        Ok(Json(AckResult { message }))
    }

    #[tool(
        description = "Show or hide ONE tile tile (persisted in `pluginsHidden`); ids are \
                       `plugin:<pluginId>:<tileId>` from bar_get_state. Hidden means off the \
                       bar only: the plugin keeps running and can still push popups; \
                       plugin_list names hidden tiles under hiddenTiles. Stop the process \
                       instead: plugin_set_active(active=false)."
    )]
    pub(super) async fn plugin_set_visible(
        &self,
        Parameters(PluginSetVisibleParams { tile_id, visible }): Parameters<PluginSetVisibleParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let changed = self
            .config
            .update(|current| {
                let hidden = current.plugins_hidden.contains(&tile_id);
                if hidden != visible {
                    return (current.clone(), false);
                }
                let mut updated = current.clone();
                if visible {
                    updated.plugins_hidden.retain(|id| *id != tile_id);
                } else {
                    updated.plugins_hidden.push(tile_id.clone());
                }
                (updated, true)
            })
            .map_err(apply_error)?;
        if !changed {
            let state = if visible { "visible" } else { "hidden" };
            return Ok(Json(AckResult {
                message: format!("tile \"{tile_id}\" was already {state}; nothing changed"),
            }));
        }
        let message = if visible {
            format!("tile \"{tile_id}\" is shown again")
        } else {
            format!(
                "tile \"{tile_id}\" is hidden; its plugin keeps running — use \
                 plugin_set_active to stop the process"
            )
        };
        Ok(Json(AckResult { message }))
    }

    #[tool(
        description = "PERMANENTLY delete a plugin — THIS CANNOT BE UNDONE: the code folder, \
                       the data directory (~/.smabar/data/<id>/), the log file and its entries \
                       in pluginsDeactivated, pluginOrder and pluginsHidden; only its \
                       settings block in config.plugins survives for a reinstall. Ask the user \
                       first. Reversible alternative: plugin_set_active(active=false)."
    )]
    pub(super) async fn plugin_remove(
        &self,
        Parameters(PluginIdParams { id }): Parameters<PluginIdParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let removal = self.supervisor.remove(&id).await.map_err(remove_error)?;
        let swept = if removal.swept.contains(&id) {
            "; its data directory and log file went with it"
        } else {
            "; it had no data directory or log file"
        };
        let reactivated = if removal.deactivation_cleared {
            " (it was deactivated; that entry was dropped too)"
        } else {
            ""
        };
        // Say what was cleaned out of the id lists: an agent that later
        // reinstalls this id needs to know the slate is clean.
        let order = match removal.order_entries_cleared.len() {
            0 => String::new(),
            count => format!(
                "; {count} tile id(s) dropped from pluginOrder/pluginsHidden: {}",
                removal.order_entries_cleared.join(", ")
            ),
        };
        Ok(Json(AckResult {
            message: format!(
                "permanently deleted plugin \"{id}\" from {}{swept}{reactivated}{order}",
                removal.dir.display()
            ),
        }))
    }
}

/// Caller mistakes become invalid_params; only a filesystem failure is an
/// internal error.
fn remove_error(error: RemoveError) -> McpError {
    match error {
        RemoveError::Io { .. } => McpError::internal_error(error.to_string(), None),
        _ => McpError::invalid_params(error.to_string(), None),
    }
}
