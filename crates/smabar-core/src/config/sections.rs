//! Config section types: layout, popups, shortcuts, and effects (appearance
//! lives in [`super::appearance`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::MonitorPreference;

/// Screen edge the bar occupies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    Top,
    #[default]
    Bottom,
}

/// How the shortcut and tile zones share the bar.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BarVariant {
    /// One row, both zones side by side with a draggable divider.
    #[default]
    Split,
    /// Two stacked rows, one zone each.
    Rows,
    /// Only the primary zone is visible; the other slides out on demand.
    Solo,
}

/// Which zone is the primary one (relevant for `rows` and `solo`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ZoneKind {
    Shortcuts,
    #[default]
    Plugins,
}

/// Horizontal extent of the bar.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BarWidth {
    /// Spans the whole screen edge (classic taskbar).
    Full,
    /// Shrinks to its content and floats centered — the dock look.
    #[default]
    Auto,
}

/// How the bar interacts with other windows and the docked screen edge.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum LayoutBehavior {
    /// Reserve screen space so normal windows do not overlap the bar.
    #[default]
    Reserve,
    /// Do not reserve screen space; stacking follows `zOrder`.
    Float,
    /// Do not reserve screen space and reveal the bar above ordinary windows.
    Autohide,
}

/// Screen corner/edge where proactive plugin popups stack up.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum PopupPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    #[default]
    BottomRight,
}

/// Plugin popup settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct PopupsConfig {
    pub enabled: bool,
    /// Where the popup stack docks on screen.
    pub position: PopupPosition,
}

impl Default for PopupsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: PopupPosition::default(),
        }
    }
}

/// Bar layout: position, zone variant, and zone sizing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LayoutConfig {
    /// Preferred physical display; `None` follows the OS primary display.
    pub monitor: Option<MonitorPreference>,
    /// Screen edge the bar docks to.
    pub position: BarPosition,
    /// Zone arrangement: split, rows, or solo.
    pub variant: BarVariant,
    /// Fraction of the bar width given to the first zone in `split`.
    /// Validated to 0.15–0.85; consumers still clamp defensively.
    pub divider_ratio: f64,
    /// Main row in `rows`; the always-visible zone in `solo`.
    pub primary_zone: ZoneKind,
    /// Full-width strip or content-sized centered dock.
    pub width: BarWidth,
    /// Maximum width in pixels when `width` is `full`; zero means unlimited.
    /// Validated to zero or 400–2400; consumers cap it to the active display.
    pub max_width: u32,
    /// Gap between the bar and the screen edges, validated to 0–64 pixels.
    pub margin: u32,
    /// Space reservation and auto-hide mode.
    pub behavior: LayoutBehavior,
    /// Let focused fullscreen applications cover the bar. Transient smabar
    /// surfaces still elevate while they are open.
    pub yield_to_fullscreen: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            monitor: None,
            position: BarPosition::default(),
            variant: BarVariant::default(),
            divider_ratio: 0.5,
            primary_zone: ZoneKind::default(),
            width: BarWidth::default(),
            max_width: 0,
            margin: 10,
            behavior: LayoutBehavior::default(),
            yield_to_fullscreen: true,
        }
    }
}

/// A platform-neutral special item exposed by the shortcut zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SpecialShortcut {
    Computer,
    Trash,
}

/// One pinned shortcut or visual separator. Launchable entries carry exactly
/// one of `desktop_id`, `path`, `url` or `special`; separators carry none.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ShortcutEntry {
    /// Generated pin id (`sc-<hash>` for launchable entries).
    pub id: String,
    /// XDG desktop-file id, e.g. `firefox.desktop`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_id: Option<String>,
    /// Absolute path to an existing file or folder opened through the native
    /// desktop association. Linux `.desktop` files keep their parsed app launch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// http(s) website opened in the system browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Curated system item opened through the native platform adapter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special: Option<SpecialShortcut>,
    /// Display label override; defaults to the app/path/system-item name or website host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Visual divider in the shortcut zone; it has no launch source.
    #[serde(default, skip_serializing_if = "is_false")]
    pub separator: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Where a shortcut's text label is rendered relative to its icon.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum LabelMode {
    Right,
    Below,
    #[default]
    Hidden,
}

/// The pinned-shortcut zone configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ShortcutsConfig {
    /// Pinned shortcut entries in display order.
    pub pinned: Vec<ShortcutEntry>,
    /// Label placement next to the shortcut icons.
    pub labels: LabelMode,
    /// Icon edge length in pixels, validated to 16–64.
    pub icon_size: u32,
    /// Label font size in pixels, validated to 9–16.
    pub label_size: u32,
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            pinned: Vec::new(),
            labels: LabelMode::default(),
            icon_size: 24,
            label_size: 12,
        }
    }
}

/// Hover-magnify effect on shortcut icons (dock-style).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HoverMagnify {
    pub enabled: bool,
    /// Scale factor of the hovered icon, validated to 1.0–1.6.
    pub scale: f64,
    /// How many neighbor icons the fisheye falloff spans on each side,
    /// validated to 0–3 (0 = only the hovered icon).
    pub neighbors: u32,
}

impl Default for HoverMagnify {
    fn default() -> Self {
        Self {
            enabled: true,
            scale: 1.2,
            neighbors: 2,
        }
    }
}

/// Read-only flyout preview shown after hovering a tile tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HoverPeek {
    pub enabled: bool,
    /// Delay before opening, validated to 100–2000 milliseconds.
    pub delay_ms: u32,
}

impl Default for HoverPeek {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: 400,
        }
    }
}

/// Visual effects settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct EffectsConfig {
    pub hover_magnify: HoverMagnify,
    pub hover_peek: HoverPeek,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_documented_values() {
        let layout = LayoutConfig::default();
        assert_eq!(layout.position, BarPosition::Bottom);
        assert_eq!(layout.variant, BarVariant::Split);
        assert!((layout.divider_ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(layout.primary_zone, ZoneKind::Plugins);
        assert_eq!(layout.width, BarWidth::Auto);
        assert_eq!(layout.max_width, 0);
        assert_eq!(layout.margin, 10);
        assert_eq!(layout.behavior, LayoutBehavior::Reserve);
        assert!(layout.yield_to_fullscreen);

        assert!(PopupsConfig::default().enabled);
        assert_eq!(PopupsConfig::default().position, PopupPosition::BottomRight);

        let shortcuts = ShortcutsConfig::default();
        assert!(shortcuts.pinned.is_empty());
        assert_eq!(shortcuts.labels, LabelMode::Hidden);
        assert_eq!(shortcuts.icon_size, 24);
        assert_eq!(shortcuts.label_size, 12);

        let effects = EffectsConfig::default();
        assert!(effects.hover_magnify.enabled);
        assert!((effects.hover_magnify.scale - 1.2).abs() < f64::EPSILON);
        assert_eq!(effects.hover_magnify.neighbors, 2);
        assert!(effects.hover_peek.enabled);
        assert_eq!(effects.hover_peek.delay_ms, 400);
    }

    #[test]
    fn sections_serialize_camel_case_and_lowercase_enums() {
        let json = serde_json::to_value(LayoutConfig::default()).expect("serialize layout");
        assert_eq!(
            json,
            serde_json::json!({
                "monitor": null,
                "position": "bottom",
                "variant": "split",
                "dividerRatio": 0.5,
                "primaryZone": "plugins",
                "width": "auto",
                "maxWidth": 0,
                "margin": 10,
                "behavior": "reserve",
                "yieldToFullscreen": true,
            })
        );

        let entry = ShortcutEntry {
            id: "sc-1234abcd".to_string(),
            desktop_id: Some("firefox.desktop".to_string()),
            path: None,
            url: None,
            special: None,
            label: None,
            separator: false,
        };
        let json = serde_json::to_value(&entry).expect("serialize entry");
        assert_eq!(
            json,
            serde_json::json!({ "id": "sc-1234abcd", "desktopId": "firefox.desktop" })
        );

        assert_eq!(
            serde_json::to_value(PopupsConfig::default()).expect("serialize popups"),
            serde_json::json!({ "enabled": true, "position": "bottom-right" })
        );
        assert_eq!(
            serde_json::to_value(EffectsConfig::default()).expect("serialize effects"),
            serde_json::json!({
                "hoverMagnify": { "enabled": true, "scale": 1.2, "neighbors": 2 },
                "hoverPeek": { "enabled": true, "delayMs": 400 },
            })
        );
    }

    #[test]
    fn sections_roundtrip_through_json() {
        let layout = LayoutConfig {
            monitor: None,
            position: BarPosition::Top,
            variant: BarVariant::Solo,
            divider_ratio: 0.5,
            primary_zone: ZoneKind::Shortcuts,
            width: BarWidth::Auto,
            max_width: 960,
            margin: 24,
            behavior: LayoutBehavior::Float,
            yield_to_fullscreen: false,
        };
        let json = serde_json::to_string(&layout).expect("serialize");
        let back: LayoutConfig = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, layout);

        let shortcuts = ShortcutsConfig {
            pinned: vec![ShortcutEntry {
                id: "sc-1".to_string(),
                desktop_id: None,
                path: Some(PathBuf::from("/opt/app/app.desktop")),
                url: None,
                special: None,
                label: Some("App".to_string()),
                separator: false,
            }],
            labels: LabelMode::Below,
            icon_size: 48,
            label_size: 10,
        };
        let json = serde_json::to_string(&shortcuts).expect("serialize");
        let back: ShortcutsConfig = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, shortcuts);
    }

    #[test]
    fn missing_section_fields_use_defaults_but_unknown_fields_are_rejected() {
        let layout: LayoutConfig = serde_json::from_str(r#"{"position":"top"}"#).expect("layout");
        assert_eq!(layout.position, BarPosition::Top);
        assert_eq!(layout.variant, BarVariant::Split);
        assert_eq!(layout.width, BarWidth::Auto);
        assert_eq!(layout.max_width, 0);
        assert_eq!(layout.margin, 10);
        assert_eq!(layout.behavior, LayoutBehavior::Reserve);
        assert!(layout.yield_to_fullscreen);

        let shortcuts: ShortcutsConfig =
            serde_json::from_str(r#"{"pinned":[]}"#).expect("shortcuts");
        assert_eq!(shortcuts.labels, LabelMode::Hidden);
        assert_eq!(shortcuts.icon_size, 24);
        assert_eq!(shortcuts.label_size, 12);

        let effects: EffectsConfig =
            serde_json::from_str(r#"{"hoverMagnify":{"scale":1.4}}"#).expect("effects");
        assert!(effects.hover_magnify.enabled);
        assert!((effects.hover_magnify.scale - 1.4).abs() < f64::EPSILON);
        assert_eq!(effects.hover_magnify.neighbors, 2);
        assert_eq!(effects.hover_peek, HoverPeek::default());

        let popups: PopupsConfig = serde_json::from_str("{}").expect("default popups");
        assert_eq!(popups, PopupsConfig::default());
        let popups: PopupsConfig =
            serde_json::from_str(r#"{"position":"top-center"}"#).expect("popups");
        assert!(popups.enabled);
        assert_eq!(popups.position, PopupPosition::TopCenter);
        assert!(serde_json::from_str::<LayoutConfig>(r#"{"positon":"top"}"#).is_err());

        let entry: ShortcutEntry = serde_json::from_str(r#"{"id":"sc-current"}"#).expect("parse");
        assert!(!entry.separator);
        assert!(entry.special.is_none());
        let separator = ShortcutEntry {
            id: "sc-separator".to_string(),
            separator: true,
            ..ShortcutEntry::default()
        };
        assert_eq!(
            serde_json::to_value(separator).expect("serialize separator"),
            serde_json::json!({ "id": "sc-separator", "separator": true })
        );

        for (value, special) in [
            ("computer", SpecialShortcut::Computer),
            ("trash", SpecialShortcut::Trash),
        ] {
            let entry: ShortcutEntry = serde_json::from_value(serde_json::json!({
                "id": format!("sc-{value}"),
                "special": value,
            }))
            .expect("parse special shortcut");
            assert_eq!(entry.special, Some(special));
            assert_eq!(
                serde_json::to_value(entry).expect("serialize special shortcut")["special"],
                value
            );
        }
    }
}
