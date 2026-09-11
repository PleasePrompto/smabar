//! Global appearance settings: bar and tile chrome, zone alignment, and
//! per-token theme overrides.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Background chrome used for tile tiles.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum TileChrome {
    #[default]
    Card,
    Flat,
}

/// Chrome of the bar surface itself. `Flat` drops the outline AND the drop
/// shadow — the inset highlights inside the shadow would otherwise stay
/// behind as a visible hairline, so a "no border" that keeps them is not a
/// flat design. The background and its opacity are unaffected.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BarChrome {
    #[default]
    Card,
    Flat,
}

/// Horizontal alignment of the tiles inside a bar zone.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ZoneAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Global appearance settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct AppearanceConfig {
    /// Outline + shadow of the bar surface.
    pub bar_chrome: BarChrome,
    pub tile_chrome: TileChrome,
    /// Alignment of the pinned shortcuts inside their zone.
    pub shortcut_align: ZoneAlign,
    /// Alignment of the tile tiles inside their zone.
    pub plugin_align: ZoneAlign,
    /// Per-token theme overrides (`--sb-*` custom properties), applied by
    /// the shell AFTER the active theme — the appearance sliders write
    /// these. Values are plain CSS values; the shell sets them via CSSOM.
    pub tokens: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_card_and_centered() {
        let appearance = AppearanceConfig::default();
        assert_eq!(appearance.bar_chrome, BarChrome::Card);
        assert_eq!(appearance.tile_chrome, TileChrome::Card);
        assert_eq!(appearance.shortcut_align, ZoneAlign::Center);
        assert_eq!(appearance.plugin_align, ZoneAlign::Center);
        assert!(appearance.tokens.is_empty());
    }

    #[test]
    fn serializes_camel_case_with_lowercase_enums() {
        assert_eq!(
            serde_json::to_value(AppearanceConfig::default()).expect("serialize appearance"),
            serde_json::json!({
                "barChrome": "card",
                "tileChrome": "card",
                "shortcutAlign": "center",
                "pluginAlign": "center",
                "tokens": {},
            })
        );
    }

    #[test]
    fn roundtrips_defaults_missing_fields_and_rejects_unknown_values() {
        let appearance = AppearanceConfig {
            bar_chrome: BarChrome::Flat,
            tile_chrome: TileChrome::Flat,
            shortcut_align: ZoneAlign::Left,
            plugin_align: ZoneAlign::Right,
            tokens: BTreeMap::from([("--sb-bar-opacity".to_string(), "62%".to_string())]),
        };
        let json = serde_json::to_string(&appearance).expect("serialize");
        let back: AppearanceConfig = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, appearance);

        let partial: AppearanceConfig =
            serde_json::from_str(r#"{"tileChrome":"flat"}"#).expect("parse partial appearance");
        assert_eq!(
            partial,
            AppearanceConfig {
                tile_chrome: TileChrome::Flat,
                ..AppearanceConfig::default()
            }
        );
        let mut unknown = serde_json::to_value(AppearanceConfig::default()).expect("serialize");
        unknown["barStyle"] = serde_json::json!("platform");
        assert!(serde_json::from_value::<AppearanceConfig>(unknown).is_err());
    }

    #[test]
    fn rejects_unknown_enum_values() {
        assert!(
            serde_json::from_str::<AppearanceConfig>(r#"{"shortcutAlign":"justify"}"#).is_err()
        );
    }
}
