//! MCP tools for the pinned-shortcut zone: app search, pin management, and
//! launching.

use std::path::PathBuf;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::shortcuts::{self, AppSource, ShortcutError};

use super::SmabarMcp;
use super::bar_tools::apply_error;
use super::types::{
    AckResult, AppInfoOut, AppSearchParams, AppSearchResult, PinnedShortcutOut, ShortcutAddParams,
    ShortcutIdParams, ShortcutListResult,
};

#[tool_router(router = shortcut_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Search installed applications by name or comment, case-insensitive; an \
                       empty query lists all. Each result carries exactly one shortcut_add \
                       source: desktopId on Linux or path on Windows/macOS. No icon data."
    )]
    pub(super) async fn app_search(
        &self,
        Parameters(AppSearchParams { query }): Parameters<AppSearchParams>,
    ) -> Result<Json<AppSearchResult>, McpError> {
        let apps = self
            .shortcuts
            .search(&query)
            .map_err(shortcut_error)?
            .into_iter()
            .map(|app| {
                let (desktop_id, path) = match app.source {
                    AppSource::DesktopId(id) => (Some(id), None),
                    AppSource::Path(path) => (None, Some(path)),
                };
                AppInfoOut {
                    desktop_id,
                    path,
                    name: app.name,
                    comment: app.comment,
                }
            })
            .collect();
        Ok(Json(AppSearchResult { apps }))
    }

    #[tool(
        description = "List pinned shortcuts and separators with resolved labels. Icon bytes are \
                       omitted; iconResolved tells whether a real icon was found. Path, website, \
                       and special pins carry their source."
    )]
    pub(super) async fn shortcut_list(&self) -> Result<Json<ShortcutListResult>, McpError> {
        let config = self.config.current();
        let pinned = self
            .shortcuts
            .resolve_pinned(&config.shortcuts)
            .into_iter()
            .zip(&config.shortcuts.pinned)
            .map(|(shortcut, entry)| PinnedShortcutOut {
                id: shortcut.id,
                label: shortcut.label,
                desktop_id: shortcut.desktop_id,
                path: entry.path.clone(),
                url: entry.url.clone(),
                special: entry.special,
                icon_resolved: !shortcut.icons.is_empty(),
                separator: shortcut.separator,
            })
            .collect();
        Ok(Json(ShortcutListResult { pinned }))
    }

    #[tool(
        description = "Pin an application, file, folder, website, special item, or visual separator \
                       to the bar \
                       (applies live). Exactly ONE source: a desktop id from app_search, an \
                       absolute existing file or folder path, an http(s) url, OR \
                       special=computer|trash — a website \
                       pin opens in the system browser and shows the site's favicon. For a \
                       separator pass separator=true with no source. `label` overrides the \
                       displayed name (default: app/path/system-item name or website host). `index` \
                       inserts at that position; omitted appends."
    )]
    pub(super) async fn shortcut_add(
        &self,
        Parameters(ShortcutAddParams {
            desktop_id,
            path,
            url,
            special,
            label,
            index,
            separator,
        }): Parameters<ShortcutAddParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let entry = self
            .shortcuts
            .validated_entry_with_special(
                desktop_id,
                path.map(PathBuf::from),
                url,
                special,
                label,
                separator,
            )
            .map_err(shortcut_error)?;
        let pin_id = entry.id.clone();
        let result = self
            .config
            .update(
                |current| match shortcuts::pin_shortcut(current, entry, index) {
                    Ok(new) => (new, Ok(())),
                    Err(error) => (current.clone(), Err(error)),
                },
            )
            .map_err(apply_error)?;
        result.map_err(shortcut_error)?;
        Ok(Json(AckResult {
            message: format!("shortcut pinned with id \"{pin_id}\""),
        }))
    }

    #[tool(description = "Unpin a shortcut by its pin id (applies live).")]
    pub(super) async fn shortcut_remove(
        &self,
        Parameters(ShortcutIdParams { id }): Parameters<ShortcutIdParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let result = self
            .config
            .update(|current| match shortcuts::unpin_shortcut(current, &id) {
                Ok(new) => (new, Ok(())),
                Err(error) => (current.clone(), Err(error)),
            })
            .map_err(apply_error)?;
        result.map_err(shortcut_error)?;
        Ok(Json(AckResult {
            message: format!("shortcut \"{id}\" unpinned"),
        }))
    }

    #[tool(
        description = "Launch a pinned shortcut by its pin id (separators are rejected). This STARTS A PROCESS on the \
                       user's machine (detached — it keeps running when smabar exits), \
                       executing the parsed desktop entry or using the native platform opener \
                       for supported paths, websites, and special items. \
                       The MCP server is reachable from localhost \
                       only."
    )]
    pub(super) async fn shortcut_launch(
        &self,
        Parameters(ShortcutIdParams { id }): Parameters<ShortcutIdParams>,
    ) -> Result<Json<AckResult>, McpError> {
        let config = self.config.current();
        let entry = config
            .shortcuts
            .pinned
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| shortcut_error(ShortcutError::UnknownPin { id: id.clone() }))?;
        self.shortcuts
            .launch_pinned(entry)
            .map_err(shortcut_error)?;
        Ok(Json(AckResult {
            message: format!("shortcut \"{id}\" launched"),
        }))
    }
}

/// Bad ids/paths/sources are caller mistakes; only OS-level failures
/// (unreadable file, failed spawn) are internal errors.
fn shortcut_error(error: ShortcutError) -> McpError {
    match error {
        ShortcutError::ReadDesktopFile { .. }
        | ShortcutError::Spawn { .. }
        | ShortcutError::Platform { .. } => McpError::internal_error(error.to_string(), None),
        _ => McpError::invalid_params(error.to_string(), None),
    }
}
