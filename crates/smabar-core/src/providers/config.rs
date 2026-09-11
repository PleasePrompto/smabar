use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Kind of system data a provider samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Cpu,
    Memory,
    Disk,
    Network,
    Battery,
    Media,
    Audio,
}

impl ProviderKind {
    /// Every provider kind known by this core, in stable order. Platform
    /// advertisement is filtered by [`super::ProviderHub::available_names`].
    pub const ALL: [Self; 7] = [
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::Network,
        Self::Battery,
        Self::Media,
        Self::Audio,
    ];

    /// Stable string form, identical to the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Network => "network",
            Self::Media => "media",
            Self::Audio => "audio",
        }
    }

    /// Parses the stable string form accepted by `provider.subscribe`.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// Sampling interval used when a config passes `interval_ms == 0`.
    fn default_interval_ms(self) -> u64 {
        match self {
            Self::Audio => 500,
            Self::Media => 1_000,
            Self::Cpu | Self::Memory | Self::Network => 2_000,
            Self::Battery => 5_000,
            Self::Disk => 30_000,
        }
    }
}

/// Subscription request for one provider.
///
/// Two configs that are equal after [`ProviderConfig::normalized`] share a
/// single sampler task; the derived `Hash`/`Eq` is the dedupe key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// Sampling interval in milliseconds; `0` selects the kind's default
    /// (500 ms for audio, 1000 ms for media, 2000 ms for cpu/memory/network,
    /// 5000 ms for battery, 30000 ms for disk).
    pub interval_ms: u64,
}

/// MPRIS player action accepted by the plugin RPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
}

/// System-output actions accepted by the plugin RPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioAction {
    SetVolume(u8),
    SetMuted(bool),
}

impl MediaAction {
    /// Parses the fixed action allowlist. The platform layer maps these values
    /// to D-Bus methods; plugin input is never used as a method name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "playPause" => Some(Self::PlayPause),
            "next" => Some(Self::Next),
            "previous" => Some(Self::Previous),
            _ => None,
        }
    }
}

impl ProviderConfig {
    /// Returns the config with `interval_ms == 0` replaced by the default.
    pub fn normalized(self) -> Self {
        Self {
            kind: self.kind,
            interval_ms: if self.interval_ms == 0 {
                self.kind.default_interval_ms()
            } else {
                self.interval_ms
            },
        }
    }

    /// Stable event key of the normalized config: `"<kind>:<interval_ms>"`.
    pub fn key(&self) -> String {
        let normalized = self.normalized();
        format!("{}:{}", normalized.kind.as_str(), normalized.interval_ms)
    }

    /// Sampling interval of the normalized config.
    pub(crate) fn interval(&self) -> Duration {
        Duration::from_millis(self.normalized().interval_ms)
    }
}
