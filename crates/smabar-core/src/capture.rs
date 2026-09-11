//! The bar's own view of itself: screenshots of the live webview and the UI
//! state needed to produce them.
//!
//! Only the shell knows the layout, only the Tauri crate can snapshot the
//! webview, and this crate may know neither. So MCP tools state an intent
//! here and the app answers it — one channel, request/response, the same
//! mpsc+oneshot shape `plugins::rpc` uses for core→plugin calls.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::platform::Rect;

/// How long the app gets to answer. Generous: a snapshot walks the whole
/// document and the shell has to lay out an expanded capture stage first.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Largest edge of a rendered capture, in device pixels. A screenshot is
/// meant to be looked at, not to be a wallpaper.
pub const MAX_CAPTURE_EDGE: u32 = 6000;

/// UI state an agent can set before taking a picture of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    /// Pin a tile's flyout open. Needs a tile id.
    OpenFlyout,
    /// Close the flyout, whichever tile owns it.
    CloseFlyout,
    /// Open the secondary row in the bar window (solo layout).
    OpenOverlay,
    CloseOverlay,
    OpenSettings,
    CloseSettings,
}

impl UiAction {
    /// Only `open_flyout` addresses a specific tile.
    pub fn needs_tile(self) -> bool {
        self == Self::OpenFlyout
    }

    /// Surface the next screenshot can prepare in the same transaction.
    pub fn capture_target(self) -> Option<&'static str> {
        match self {
            Self::OpenFlyout => Some("flyout"),
            Self::OpenOverlay => Some("overlay"),
            Self::OpenSettings => Some("settings"),
            Self::CloseFlyout | Self::CloseOverlay | Self::CloseSettings => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BarRequest {
    Screenshot {
        target: String,
        scale: u8,
    },
    UiState {
        action: UiAction,
        tile_id: Option<String>,
        /// Settings group or page to open (`bar`, `design`, `shortcuts`,
        /// `plugins`, `system`, `legal`, `plugins/store`, `design/themes`);
        /// only `open_settings` reads it.
        group: Option<String>,
    },
}

/// What the shell measured, plus the rendered image.
#[derive(Debug, Clone)]
pub struct Screenshot {
    pub png: Vec<u8>,
    /// The subject's rectangle in CSS pixels, as the shell measured it.
    pub rect: Rect,
    /// Size of the returned PNG in device pixels.
    pub pixels: (u32, u32),
    /// True when clipping was lifted to fit scrollable content into the shot.
    pub expanded: bool,
    /// True when something clips even after that: the shot is incomplete.
    pub clipped: bool,
    /// Every target the shell can currently address.
    pub targets: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum BarResponse {
    Screenshot(Box<Screenshot>),
    UiState { targets: Vec<String> },
}

#[derive(Debug, Error)]
pub enum BarError {
    /// The shell has no such target; the message lists what it does have.
    #[error("unknown target `{target}` — available: {}", available.join(", "))]
    UnknownTarget {
        target: String,
        available: Vec<String>,
    },
    #[error("the bar could not take the picture: {0}")]
    Failed(String),
    #[error("the bar is not running or its window is gone")]
    Disconnected,
    #[error("the bar did not answer within {0:?}")]
    Timeout(Duration),
    /// Screenshots need the platform's webview snapshot API.
    #[error("screenshots are not supported on this platform")]
    Unsupported,
}

type Envelope = (BarRequest, oneshot::Sender<Result<BarResponse, BarError>>);

/// The MCP side of the channel. Cloneable, cheap, and inert until the app
/// installs a receiver — a tool call then fails with `Disconnected` rather
/// than hanging, which is the honest answer when no window is up.
#[derive(Debug, Clone)]
pub struct BarPort {
    tx: mpsc::UnboundedSender<Envelope>,
}

impl BarPort {
    /// Returns the port and the receiver the app's capture task drains.
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<Envelope>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub async fn send(&self, request: BarRequest) -> Result<BarResponse, BarError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send((request, reply_tx))
            .map_err(|_| BarError::Disconnected)?;
        match tokio::time::timeout(REQUEST_TIMEOUT, reply_rx).await {
            Ok(Ok(result)) => result,
            // The app dropped the sender: it crashed or is shutting down.
            Ok(Err(_)) => Err(BarError::Disconnected),
            Err(_) => Err(BarError::Timeout(REQUEST_TIMEOUT)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_a_response() {
        let (port, mut rx) = BarPort::channel();
        tokio::spawn(async move {
            let (request, reply) = rx.recv().await.expect("request");
            assert!(matches!(request, BarRequest::Screenshot { .. }));
            let _ = reply.send(Ok(BarResponse::UiState {
                targets: vec!["bar".into()],
            }));
        });
        let response = port
            .send(BarRequest::Screenshot {
                target: "bar".into(),
                scale: 1,
            })
            .await
            .expect("response");
        assert!(matches!(response, BarResponse::UiState { .. }));
    }

    #[tokio::test]
    async fn reports_a_missing_app_instead_of_hanging() {
        let (port, rx) = BarPort::channel();
        drop(rx);
        let error = port
            .send(BarRequest::UiState {
                action: UiAction::CloseFlyout,
                tile_id: None,
                group: None,
            })
            .await
            .expect_err("no receiver");
        assert!(matches!(error, BarError::Disconnected));
    }

    #[tokio::test]
    async fn reports_a_dropped_reply_channel() {
        let (port, mut rx) = BarPort::channel();
        tokio::spawn(async move {
            let (_request, reply) = rx.recv().await.expect("request");
            drop(reply);
        });
        let error = port
            .send(BarRequest::UiState {
                action: UiAction::OpenSettings,
                tile_id: None,
                group: None,
            })
            .await
            .expect_err("dropped reply");
        assert!(matches!(error, BarError::Disconnected));
    }

    #[test]
    fn only_open_flyout_addresses_a_tile() {
        assert!(UiAction::OpenFlyout.needs_tile());
        assert!(!UiAction::CloseFlyout.needs_tile());
        assert!(!UiAction::OpenSettings.needs_tile());
    }

    #[test]
    fn only_open_actions_prepare_a_capture_target() {
        assert_eq!(UiAction::OpenFlyout.capture_target(), Some("flyout"));
        assert_eq!(UiAction::OpenOverlay.capture_target(), Some("overlay"));
        assert_eq!(UiAction::OpenSettings.capture_target(), Some("settings"));
        assert_eq!(UiAction::CloseFlyout.capture_target(), None);
        assert_eq!(UiAction::CloseOverlay.capture_target(), None);
        assert_eq!(UiAction::CloseSettings.capture_target(), None);
    }
}
