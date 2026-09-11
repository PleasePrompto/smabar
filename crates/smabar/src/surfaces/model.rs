use serde::{Deserialize, Serialize};
use smabar_core::platform::surfaces::{FlyoutDirection, ScreenPoint, ScreenRect};

/// Fixed webview roles. Labels are internal contracts, not user configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceRole {
    Bar,
    Overlay,
    Settings,
    Notifications,
}

impl SurfaceRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Overlay => "overlay",
            Self::Settings => "settings",
            Self::Notifications => "notifications",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "bar" => Some(Self::Bar),
            "overlay" => Some(Self::Overlay),
            "settings" => Some(Self::Settings),
            "notifications" => Some(Self::Notifications),
            _ => None,
        }
    }

    pub const fn plugin_capable(self) -> bool {
        matches!(self, Self::Bar | Self::Overlay | Self::Notifications)
    }

    pub const fn persistent(self) -> bool {
        matches!(self, Self::Bar | Self::Overlay | Self::Settings)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceContext {
    pub role: SurfaceRole,
    pub work_area_width: f64,
    pub work_area_height: f64,
    pub settings_open: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRequest {
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeRequest {
    pub key: String,
    pub ttl_ms: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub fn validate(self) -> Result<Self, String> {
        if self.width == 0 || self.height == 0 || self.width > 16_384 || self.height > 16_384 {
            return Err("surface size must be between 1 and 16384 CSS pixels".to_string());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationMeasure {
    pub popup: Option<SurfaceSize>,
    pub notice: Option<SurfaceSize>,
    pub edge_inset: u32,
    pub gap: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPlacement {
    pub popup: Option<LocalPoint>,
    pub notice: Option<LocalPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlyoutMode {
    Peek,
    Pinned,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayFlyoutRequest {
    pub generation: u64,
    pub tile_id: String,
    pub mode: FlyoutMode,
    /// An in-place pin keeps the rendered DOM and its native presentation.
    pub preserve_content: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayMeasure {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub inset: u32,
    pub gap: u32,
    pub pointer_reserve: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPlacement {
    pub generation: u64,
    pub direction: &'static str,
    pub pointer_x: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveFlyout {
    pub request: OverlayFlyoutRequest,
    pub trigger: ScreenRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FlyoutLayout {
    pub frame: ScreenRect,
    pub direction: FlyoutDirection,
    pub pointer_x: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMenuRequest {
    pub generation: u64,
    pub items: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveMenu {
    pub request: OverlayMenuRequest,
    pub anchor: ScreenPoint,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuMeasure {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub inset: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuPlacement {
    pub generation: u64,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayTooltipRequest {
    pub generation: u64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveTooltip {
    pub request: OverlayTooltipRequest,
    pub trigger: ScreenRect,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TooltipMeasure {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub inset: u32,
    pub gap: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TooltipLayout {
    pub frame: ScreenRect,
    pub direction: FlyoutDirection,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TooltipPlacement {
    pub generation: u64,
    pub side: &'static str,
    pub x: f64,
    pub y: f64,
}

#[cfg(test)]
mod tests {
    use super::SurfaceRole;

    #[test]
    fn labels_roundtrip() {
        for role in [
            SurfaceRole::Bar,
            SurfaceRole::Overlay,
            SurfaceRole::Settings,
            SurfaceRole::Notifications,
        ] {
            assert_eq!(SurfaceRole::from_label(role.label()), Some(role));
        }
        assert_eq!(SurfaceRole::from_label("unknown"), None);
    }
}
