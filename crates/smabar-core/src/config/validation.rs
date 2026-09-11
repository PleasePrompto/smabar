//! Semantic config validation shared by every load and write path.

use thiserror::Error;

use super::SmabarConfig;

const DIVIDER_MIN: f64 = 0.15;
const DIVIDER_MAX: f64 = 0.85;

/// A config value that has the right JSON type but is unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct ConfigValidationError(&'static str);

pub(super) fn validate(config: &SmabarConfig) -> Result<(), ConfigValidationError> {
    require(
        config.audio.volume <= 100
            && config
                .audio
                .plugins
                .values()
                .all(|level| level.volume <= 100),
        "audio volumes must be integers from 0 to 100",
    )?;
    require(
        config
            .audio
            .plugins
            .keys()
            .all(|id| crate::plugins::is_valid_plugin_id(id)),
        "audio plugin keys must be valid plugin ids",
    )?;
    require(
        crate::i18n::is_valid_language_code(&config.language),
        "language must be a non-empty file stem containing only letters, digits, '-' or '_'",
    )?;
    require(
        crate::themes::is_valid_theme_name(&config.theme),
        "theme must be a non-empty file stem containing only lowercase letters, digits or '-'",
    )?;
    require(
        config.mcp.port >= 1_024,
        "mcp.port must be between 1024 and 65535",
    )?;
    require(
        config.layout.divider_ratio.is_finite()
            && (DIVIDER_MIN..=DIVIDER_MAX).contains(&config.layout.divider_ratio),
        "layout.dividerRatio must be a finite number between 0.15 and 0.85",
    )?;
    require(
        config.layout.max_width == 0 || (400..=2_400).contains(&config.layout.max_width),
        "layout.maxWidth must be 0 (unlimited) or between 400 and 2400",
    )?;
    require(
        config.layout.margin <= 64,
        "layout.margin must be between 0 and 64",
    )?;
    require(
        (16..=64).contains(&config.shortcuts.icon_size),
        "shortcuts.iconSize must be between 16 and 64",
    )?;
    require(
        (9..=16).contains(&config.shortcuts.label_size),
        "shortcuts.labelSize must be between 9 and 16",
    )?;
    require(
        config.effects.hover_magnify.scale.is_finite()
            && (1.0..=1.6).contains(&config.effects.hover_magnify.scale),
        "effects.hoverMagnify.scale must be a finite number between 1 and 1.6",
    )?;
    require(
        config.effects.hover_magnify.neighbors <= 3,
        "effects.hoverMagnify.neighbors must be between 0 and 3",
    )?;
    require(
        (100..=2_000).contains(&config.effects.hover_peek.delay_ms),
        "effects.hoverPeek.delayMs must be between 100 and 2000",
    )?;
    require(
        config.settings_window.width >= 640,
        "settingsWindow.width must be at least 640",
    )?;
    require(
        config.settings_window.height >= 480,
        "settingsWindow.height must be at least 480",
    )
}

fn require(valid: bool, message: &'static str) -> Result<(), ConfigValidationError> {
    valid.then_some(()).ok_or(ConfigValidationError(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_each_unsupported_semantic_value() {
        type InvalidCase = (&'static str, fn(&mut SmabarConfig));
        let invalid: [InvalidCase; 13] = [
            ("language", |config: &mut SmabarConfig| {
                config.language.clear()
            }),
            ("theme", |config: &mut SmabarConfig| {
                config.theme = "../bad".to_string()
            }),
            ("mcp.port", |config: &mut SmabarConfig| config.mcp.port = 0),
            ("layout.dividerRatio", |config: &mut SmabarConfig| {
                config.layout.divider_ratio = 0.9
            }),
            ("layout.maxWidth", |config: &mut SmabarConfig| {
                config.layout.max_width = 2_401
            }),
            ("layout.margin", |config: &mut SmabarConfig| {
                config.layout.margin = 65
            }),
            ("shortcuts.iconSize", |config: &mut SmabarConfig| {
                config.shortcuts.icon_size = 15
            }),
            ("shortcuts.labelSize", |config: &mut SmabarConfig| {
                config.shortcuts.label_size = 17
            }),
            ("effects.hoverMagnify.scale", |config: &mut SmabarConfig| {
                config.effects.hover_magnify.scale = f64::NAN
            }),
            (
                "effects.hoverMagnify.neighbors",
                |config: &mut SmabarConfig| config.effects.hover_magnify.neighbors = 4,
            ),
            ("effects.hoverPeek.delayMs", |config: &mut SmabarConfig| {
                config.effects.hover_peek.delay_ms = 99
            }),
            ("settingsWindow.width", |config: &mut SmabarConfig| {
                config.settings_window.width = 639
            }),
            ("settingsWindow.height", |config: &mut SmabarConfig| {
                config.settings_window.height = 479
            }),
        ];

        for (path, mutate) in invalid {
            let mut config = SmabarConfig::default();
            mutate(&mut config);
            let error = config.validate().expect_err(path);
            assert!(error.to_string().contains(path), "{path}: {error}");
        }
    }
}
