//! MCP tools that let an agent SEE the bar it is building: screenshots of the
//! live window, and the UI state needed to expose what should be in them.

use base64::Engine as _;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::capture::{BarError, BarRequest, BarResponse, Screenshot};

use super::SmabarMcp;
use super::types::{BarScreenshotParams, BarUiStateParams};

/// Upper bound on the caller-supplied magnification.
const MAX_SCALE: u8 = 4;

#[tool_router(router = capture_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Screenshot of the bar itself — a PNG of the live window, nothing else on \
                       the desktop. Omit `target` for the whole bar, or pass a tile id from \
                       bar_get_state (\"plugin:<pluginId>:<tileId>\"), \
                       \"shortcut:<id>\", or an open surface (\"flyout\", \"overlay\", \
                       \"settings\", \"popup\"). \"overlay\" captures the visible secondary row \
                       in the solo layout. Scrollable content is expanded first, so a long \
                       flyout comes back complete instead of cut off at its scroll edge. Use \
                       scale 2-4 for small tiles; Windows magnifies its natural-resolution \
                       WebView2 preview, so larger scales add no detail. An unknown target \
                       answers with the list of targets that exist right now. Open a tile's \
                       flyout with bar_ui_state immediately before shooting it; the paired shot \
                       re-applies that open state through a brief render gap while a Plugin \
                       restarts."
    )]
    pub(super) async fn bar_screenshot(
        &self,
        Parameters(BarScreenshotParams { target, scale }): Parameters<BarScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let scale = scale.unwrap_or(1).clamp(1, MAX_SCALE);
        let target = target.unwrap_or_else(|| "bar".to_string());
        let response = self
            .bar
            .send(BarRequest::Screenshot {
                target: target.clone(),
                scale,
            })
            .await
            .map_err(bar_error)?;
        let BarResponse::Screenshot(shot) = response else {
            return Err(McpError::internal_error(
                "the bar answered a screenshot request with something else",
                None,
            ));
        };
        Ok(CallToolResult::success(vec![
            ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(&shot.png),
                "image/png",
            ),
            ContentBlock::text(describe(&target, &shot)),
        ]))
    }

    #[tool(
        description = "Open or close a bar surface so it can be screenshotted: open_flyout \
                       (needs tileId), close_flyout, open_overlay (secondary row; requires \
                       the solo layout), close_overlay, \
                       open_settings (optionally with `group`; see the parameter schema for pages), close_settings. A successful open also prepares the \
                       immediately following screenshot of that surface, so a brief render gap \
                       while a Plugin restarts does not lose it."
    )]
    pub(super) async fn bar_ui_state(
        &self,
        Parameters(BarUiStateParams {
            action,
            tile_id,
            group,
        }): Parameters<BarUiStateParams>,
    ) -> Result<CallToolResult, McpError> {
        if action.needs_tile() && tile_id.as_deref().unwrap_or("").is_empty() {
            return Err(McpError::invalid_params(
                format!("{action:?} needs a tileId — take one from bar_get_state"),
                None,
            ));
        }
        let response = self
            .bar
            .send(BarRequest::UiState {
                action,
                tile_id,
                group,
            })
            .await
            .map_err(bar_error)?;
        let BarResponse::UiState { targets } = response else {
            return Err(McpError::internal_error(
                "the bar answered a ui state request with something else",
                None,
            ));
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "applied. screenshot targets now available: {}",
            targets.join(", ")
        ))]))
    }
}

/// The text block beside the image: what was shot, and whether the picture
/// shows an expanded view rather than what a user sees.
fn describe(target: &str, shot: &Screenshot) -> String {
    let (width, height) = shot.pixels;
    let mut text = format!(
        "target: {target}\ncss rect: {}x{} at ({}, {})\npng: {width}x{height}",
        shot.rect.w, shot.rect.h, shot.rect.x, shot.rect.y
    );
    if shot.expanded {
        text.push_str(
            "\nnote: scrollable content was expanded for this shot, so it shows the full \
             content, not the scrolled-in-view part",
        );
    }
    if shot.clipped {
        text.push_str(
            "\nwarning: something in this subject still clips its content, so the picture \
             is incomplete — a fixed height on a scroll container will do that",
        );
    }
    text.push_str(&format!("\navailable targets: {}", shot.targets.join(", ")));
    text
}

/// A missing target or a closed surface is the caller's mistake and answers
/// with what IS there; everything else is a fault of the bar.
fn bar_error(error: BarError) -> McpError {
    match error {
        BarError::UnknownTarget { .. } => McpError::invalid_params(error.to_string(), None),
        _ => McpError::internal_error(error.to_string(), None),
    }
}

/// Kept so a new action cannot be added without deciding whether it needs a
/// tile id (the check above is the only guard against a silent no-op).
#[cfg(test)]
mod tests {
    use crate::capture::UiAction;

    #[test]
    fn every_action_states_whether_it_needs_a_tile() {
        for action in [
            UiAction::OpenFlyout,
            UiAction::CloseFlyout,
            UiAction::OpenOverlay,
            UiAction::CloseOverlay,
            UiAction::OpenSettings,
            UiAction::CloseSettings,
        ] {
            assert_eq!(action.needs_tile(), action == UiAction::OpenFlyout);
        }
    }
}
