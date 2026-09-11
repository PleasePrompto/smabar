//! MCP access to the same font inventory used by the settings picker.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::fonts;

use super::SmabarMcp;
use super::types::{FontListParams, FontListResult};

#[tool_router(router = font_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Search installed system families and the checked-in Google Fonts catalog. \
                       Write returned `cssStack` to the slot's family token. For Google, pair it \
                       with source token `google:<catalog-id>`; \
                       smabar canonicalizes the family, downloads it locally on theme activation, \
                       and keeps its fallback until ready. For system fonts, pair `cssStack` with \
                       source token `system`; shared themes fall back on hosts where \
                       that family is unavailable. `cached` is meaningful for Google entries \
                       because system entries are already local. Results default to 50 and are \
                       capped at 100.",
        annotations(read_only_hint = true)
    )]
    pub(super) async fn font_list(
        &self,
        Parameters(FontListParams {
            query,
            source,
            monospaced,
            limit,
        }): Parameters<FontListParams>,
    ) -> Result<Json<FontListResult>, McpError> {
        let paths = self.paths.clone();
        let options = tokio::task::spawn_blocking(move || {
            fonts::font_options(&paths, query.as_deref(), source, monospaced, limit)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot scan the font inventory: {error}"), None)
        })?;
        Ok(Json(FontListResult { fonts: options }))
    }
}
