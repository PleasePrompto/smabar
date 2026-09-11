//! Host-owned audio preferences, independent of the system output volume.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct AudioLevel {
    pub volume: u8,
    pub muted: bool,
}

impl Default for AudioLevel {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct AudioConfig {
    pub volume: u8,
    pub muted: bool,
    pub notification_sounds: bool,
    pub plugins: BTreeMap<String, AudioLevel>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
            notification_sounds: true,
            plugins: BTreeMap::new(),
        }
    }
}

impl AudioConfig {
    pub fn gain(&self, plugin: &str, volume: u8, notification: bool) -> f32 {
        let level = self.plugins.get(plugin).cloned().unwrap_or_default();
        if self.muted || level.muted || (notification && !self.notification_sounds) {
            return 0.0;
        }
        f32::from(self.volume) * f32::from(level.volume) * f32::from(volume) / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{SmabarConfig, update::set_config_path};
    use serde_json::json;

    #[test]
    fn audio_settings_validate_and_multiply_without_changing_system_volume() {
        let config = set_config_path(
            &SmabarConfig::default(),
            "audio.plugins.todos.volume",
            json!(50),
        )
        .expect("plugin level");
        assert_eq!(config.audio.gain("todos", 50, false), 0.25);
        assert!(set_config_path(&config, "audio.volume", json!(101)).is_err());
        let config =
            set_config_path(&config, "audio.notificationSounds", json!(false)).expect("mute hints");
        assert_eq!(config.audio.gain("todos", 100, true), 0.0);
        assert_eq!(config.audio.gain("todos", 100, false), 0.5);
    }
}
