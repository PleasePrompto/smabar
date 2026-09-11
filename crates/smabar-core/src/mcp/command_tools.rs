//! Plugin-owned commands: discovery and acknowledged execution.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::SmabarMcp;
use super::plugin_tools::validate_plugin_id;
use super::plugin_types::PluginIdParams;
use crate::plugins::PluginCommandInfo;

#[derive(Deserialize, JsonSchema)]
pub struct CallParams {
    pub id: String,
    pub command: String,
    /// JSON object matching the command's inputSchema. Reuse requestId after
    /// an ambiguous timeout if the plugin advertises idempotent writes.
    #[schemars(schema_with = "command_arguments_schema")]
    pub arguments: Value,
}

fn command_arguments_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"object","additionalProperties":true})
}

#[derive(Serialize, JsonSchema)]
pub struct CommandsResult {
    pub commands: Vec<PluginCommandInfo>,
}

#[tool_router(router = command_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Discover a RUNNING plugin's commands, descriptions and input/output JSON Schemas. These operate on plugin data without editing code or restarting it. An empty list means the plugin has not registered commands."
    )]
    pub(super) async fn plugin_commands(
        &self,
        Parameters(PluginIdParams { id }): Parameters<PluginIdParams>,
    ) -> Result<Json<CommandsResult>, McpError> {
        validate_plugin_id(&id)?;
        let commands = self
            .supervisor
            .commands(&id)
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        Ok(Json(CommandsResult { commands }))
    }

    #[tool(
        description = "Call a discovered plugin command and await its actual result (10 s timeout). Unlike plugin_action, this confirms handler completion. The plugin validates arguments and owns persistence. A timeout or disconnection means outcome UNKNOWN, not rollback: query state or retry with the SAME requestId if supported. Use plugin_commands first."
    )]
    pub(super) async fn plugin_call(
        &self,
        Parameters(params): Parameters<CallParams>,
    ) -> Result<Json<Value>, McpError> {
        validate_plugin_id(&params.id)?;
        self.supervisor
            .call(&params.id, &params.command, params.arguments)
            .await
            .map(Json)
            .map_err(|error| McpError::invalid_params(error.to_string(), None))
    }
}
