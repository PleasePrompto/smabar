//! Bar measurement, native placement, and screen-space reservation.

use std::time::Duration;

use smabar_core::config::{BarPosition, LayoutBehavior};
use smabar_core::platform::Rect;
use tauri::{AppHandle, State, WebviewWindow};

use super::AppState;
use crate::platform;
use crate::platform::strut::DockEdge;
use crate::surfaces::{SurfaceManager, SurfaceSize};

const EDGE_RELOCATION_FRAME: Duration = Duration::from_millis(20);

/// Reports the bar's position and rect (logical window coordinates) for the
/// active display backend's screen-space reservation.
#[tauri::command]
pub async fn set_bar_geometry(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    surfaces: State<'_, SurfaceManager>,
    position: String,
    rect: Rect,
    surface: SurfaceSize,
) -> Result<(), String> {
    let (position, edge) = match position.as_str() {
        "top" => (BarPosition::Top, DockEdge::Top),
        "bottom" => (BarPosition::Bottom, DockEdge::Bottom),
        other => {
            return Err(format!(
                "unknown bar position \"{other}\"; expected \"top\" or \"bottom\""
            ));
        }
    };
    let config = state.config();
    if config.layout.position != position {
        tracing::debug!(?position, current = ?config.layout.position, "ignored stale bar geometry after edge change");
        return Ok(());
    }
    let relocation_frame = surfaces
        .begin_bar_relocation_frame(position)
        .map_err(|error| format!("{error:#}"))?;
    let relocating = relocation_frame.is_some();
    let concealed = if relocating {
        match platform::set_bar_transition_opaque(&window, false).await {
            Ok(concealed) => concealed,
            Err(error) => {
                tracing::warn!(%error, "failed to conceal bar before moving edge");
                false
            }
        }
    } else {
        false
    };
    if concealed {
        tokio::time::sleep(EDGE_RELOCATION_FRAME).await;
    }
    let dock = reservation_for_behavior(config.layout.behavior, edge, rect);
    let placement = match surfaces.update_bar_geometry(&window, rect, surface, &config) {
        // An edge change is visible at once; slider reports settle first.
        Ok(frame) if relocating => surfaces.reserve_bar_space(&app, &window, dock, frame).await,
        Ok(frame) => surfaces.defer_bar_reservation(&app, &window, dock, frame),
        Err(error) => Err(error),
    };
    if concealed {
        tokio::time::sleep(EDGE_RELOCATION_FRAME).await;
    }
    let claimed = match (relocation_frame, placement.is_ok()) {
        (Some(frame), true) => surfaces
            .claim_bar_relocation_frame(position, frame)
            .map_err(|error| format!("failed to finish bar relocation: {error:#}"))?,
        _ => false,
    };
    let visibility = if concealed && claimed {
        match platform::set_bar_transition_opaque(&window, true).await {
            Ok(_) => Ok(()),
            Err(error) => {
                surfaces
                    .restore_bar_relocation(position)
                    .map_err(|restore| {
                        format!("{error:#}; failed to restore pending relocation: {restore:#}")
                    })?;
                Err(error)
            }
        }
    } else {
        Ok(())
    };
    let errors = [
        placement.err().map(|error| format!("{error:#}")),
        visibility
            .err()
            .map(|error| format!("failed to restore bar visibility: {error:#}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn reservation_for_behavior(
    behavior: LayoutBehavior,
    edge: DockEdge,
    rect: Rect,
) -> Option<(DockEdge, Rect)> {
    match behavior {
        LayoutBehavior::Reserve => Some((edge, rect)),
        LayoutBehavior::Float | LayoutBehavior::Autohide => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: Rect = Rect {
        x: 10,
        y: 20,
        w: 800,
        h: 48,
    };

    #[test]
    fn only_reserve_behavior_creates_a_native_reservation() {
        assert_eq!(
            reservation_for_behavior(LayoutBehavior::Reserve, DockEdge::Bottom, RECT),
            Some((DockEdge::Bottom, RECT))
        );
        assert_eq!(
            reservation_for_behavior(LayoutBehavior::Float, DockEdge::Bottom, RECT),
            None
        );
        assert_eq!(
            reservation_for_behavior(LayoutBehavior::Autohide, DockEdge::Bottom, RECT),
            None
        );
    }
}
