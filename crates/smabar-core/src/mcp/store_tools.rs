//! MCP tools for the Community Store — the same `StoreService` the settings
//! panel uses, so an agent installs exactly what a user would, with the same
//! checks, receipts and rollback.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::store::{
    InstallOutcome, ItemKind, StoreDetail, StoreOverview, StoreService, ThemeInstallOutcome,
};

use super::SmabarMcp;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreDetailParams {
    pub kind: ItemKind,
    /// The listed id, as `store_overview` shows it.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreInstallPluginParams {
    /// The listed plugin id.
    pub id: String,
    /// The version `store_overview` showed; a listing that moved since is
    /// refused so nothing installs unseen.
    pub expected_version: String,
    /// Replace a locally edited copy (a backup is kept either way).
    #[serde(default)]
    pub confirm_modified: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreInstallThemeParams {
    /// The listed theme name.
    pub name: String,
    pub expected_version: String,
}

impl SmabarMcp {
    fn store(&self) -> Result<&StoreService, McpError> {
        self.store.as_ref().ok_or_else(|| {
            McpError::internal_error("the Community Store client is not available", None)
        })
    }
}

fn store_error(error: crate::store::StoreError) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

#[tool_router(router = store_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "The Community Store catalog joined with what is installed: every listed \
                       plugin and theme with version, repository, exact commit, requirements \
                       (smabar version, OS, external programs), installed state, pending \
                       update, block reason and whether it is installable here. Served from the \
                       verified cache without touching the network; `catalogState` says how \
                       current it is — call store_refresh for a fresh one."
    )]
    pub(super) async fn store_overview(&self) -> Result<Json<StoreOverview>, McpError> {
        Ok(Json(self.store()?.overview()))
    }

    #[tool(
        description = "Fetch the catalog from store.smabar.com (conditional GET), verify its \
                       signature, apply the blocklist to installed plugins and return the new \
                       overview. An unreachable store keeps the last verified catalog and reports \
                       it in `catalogState` and `lastError`."
    )]
    pub(super) async fn store_refresh(&self) -> Result<Json<StoreOverview>, McpError> {
        Ok(Json(self.store()?.refresh().await))
    }

    #[tool(
        description = "One listed item with its README (plain text, as published) and its \
                       releases, verified against the SHA-256 the listing carries."
    )]
    pub(super) async fn store_detail(
        &self,
        Parameters(StoreDetailParams { kind, id }): Parameters<StoreDetailParams>,
    ) -> Result<Json<StoreDetail>, McpError> {
        self.store()?
            .detail(kind, &id)
            .await
            .map(Json)
            .map_err(store_error)
    }

    #[tool(
        description = "Install or update a Community Plugin to its listed version: refreshes the \
                       catalog, downloads GitHub's commit archive, proves the folder against the \
                       listed git tree oid, swaps it into ~/.smabar/plugins/<id>/ (the previous \
                       version goes to a backup) and starts it. Refused for bundled ids, folders \
                       without a store receipt (the user's own plugins), blocked versions and \
                       unmet requirements; a version that fails to start is rolled back. Show \
                       the user the repository, commit and requirements from store_overview \
                       first — smabar never reviews plugin code."
    )]
    pub(super) async fn store_install_plugin(
        &self,
        Parameters(StoreInstallPluginParams {
            id,
            expected_version,
            confirm_modified,
        }): Parameters<StoreInstallPluginParams>,
    ) -> Result<Json<InstallOutcome>, McpError> {
        self.store()?
            .install_plugin(&id, &expected_version, confirm_modified, &|_| {})
            .await
            .map(Json)
            .map_err(store_error)
    }

    #[tool(
        description = "Install or update a Community Theme to its listed version: the raw file \
                       is verified against the listed SHA-256 and written to \
                       ~/.smabar/themes/<name>.json through the theme import path. Bundled \
                       names and files without a store receipt are refused."
    )]
    pub(super) async fn store_install_theme(
        &self,
        Parameters(StoreInstallThemeParams {
            name,
            expected_version,
        }): Parameters<StoreInstallThemeParams>,
    ) -> Result<Json<ThemeInstallOutcome>, McpError> {
        self.store()?
            .install_theme(&name, &expected_version)
            .await
            .map(Json)
            .map_err(store_error)
    }
}
