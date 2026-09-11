//! System provider engine: samples battery, CPU, memory, disk, network, media,
//! and system-audio data and fans it out to subscribers.
//!
//! The [`ProviderHub`] deduplicates samplers per [`ProviderConfig`] (one tokio
//! task per distinct config, refcounted), replays the last cached value to new
//! subscribers, skips emissions whose payload did not change, and stops a
//! sampler when its last [`Subscription`] is dropped. Payloads are typed
//! structs serialized to camelCase JSON.

mod battery;
mod config;
mod data;
mod hub;
mod sampler;

#[cfg(test)]
mod battery_tests;
#[cfg(test)]
mod tests;

pub use config::{AudioAction, MediaAction, ProviderConfig, ProviderKind};
pub use data::{
    AudioData, AudioOutput, BatteryData, BatteryInfo, BatteryState, CpuData, DiskMountData,
    MediaData, MediaPlaybackState, MediaSession, MemoryData, NetworkData, ProviderEvent,
};
pub use hub::{ProviderHub, Subscription};
