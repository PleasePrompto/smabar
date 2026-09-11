use serde::Serialize;
use serde_json::Value;

use super::config::ProviderKind;

/// One emission from a sampler, fanned out to all hub subscribers.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEvent {
    /// Stable identity of the emitting sampler: `"<kind>:<interval_ms>"`.
    pub key: String,
    pub kind: ProviderKind,
    /// Kind-specific payload; the JSON form of [`CpuData`], [`MemoryData`],
    /// [`BatteryData`], [`MediaData`], [`AudioData`],
    /// `Vec<`[`DiskMountData`]`>`, or [`NetworkData`].
    pub data: Value,
    /// Unix timestamp in milliseconds at sampling time.
    pub ts_ms: u64,
}

/// Stable battery state exposed to plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    Empty,
    Unknown,
}

/// Payload of [`ProviderKind::Battery`] events.
///
/// An empty `batteries` array means none are currently present, so clients can
/// remove battery UI without inferring the operating system.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryData {
    pub batteries: Vec<BatteryInfo>,
}

/// One detected battery in a [`BatteryData`] payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryInfo {
    pub charge_percent: f32,
    pub health_percent: f32,
    pub cycle_count: Option<u32>,
    pub state: BatteryState,
    pub is_charging: bool,
    pub time_till_empty: Option<f32>,
    pub time_till_full: Option<f32>,
    pub power_consumption: f32,
    pub voltage: f32,
}

/// Stable playback state exposed by an MPRIS media session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaPlaybackState {
    Playing,
    Paused,
    Stopped,
}

/// Payload of [`ProviderKind::Media`] events.
///
/// Sessions are sorted by id. `current_session_id` deterministically prefers
/// the first playing session, then paused, then stopped.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaData {
    pub current_session_id: Option<String>,
    pub sessions: Vec<MediaSession>,
}

/// Payload of [`ProviderKind::Audio`] events. `default_output: null` means
/// that the audio service is running but no output device is available.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioData {
    pub default_output: Option<AudioOutput>,
}

/// Current state of the system's default output device.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutput {
    pub name: String,
    pub volume_percent: f32,
    pub muted: bool,
}

/// One MPRIS player session. All text is plain, bounded data; plugins must
/// HTML-escape it before interpolation just like any other provider value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSession {
    pub id: String,
    pub identity: String,
    pub desktop_entry: Option<String>,
    pub playback_state: MediaPlaybackState,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

/// Payload of [`ProviderKind::Cpu`] events.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuData {
    /// Total CPU usage in percent (average over all cores, 0–100).
    pub usage_percent: f32,
    /// Per-core usage in percent, in `sysinfo` core order.
    pub per_core: Vec<f32>,
    pub core_count: usize,
    /// Highest current core frequency in MHz.
    pub frequency_mhz: u64,
}

/// Payload of [`ProviderKind::Memory`] events.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryData {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f32,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
}

/// One mounted filesystem in a [`ProviderKind::Disk`] payload (the event
/// payload is an array of these, pseudo filesystems filtered out).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMountData {
    pub mount_point: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f32,
}

/// Payload of [`ProviderKind::Network`] events: throughput summed over all
/// non-loopback interfaces since the previous sample. The first sample after
/// a sampler starts has no baseline and reports 0.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkData {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
}
