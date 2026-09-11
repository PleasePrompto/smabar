use std::sync::{Arc, Mutex, OnceLock};

use crate::util::lock_unpoisoned;
use std::time::Instant;

use serde_json::Value;
use sysinfo::{CpuRefreshKind, Disks, MINIMUM_CPU_UPDATE_INTERVAL, Networks, System};

use crate::platform::{
    AUDIO_SUPPORTED, AudioError, AudioSource, MEDIA_SUPPORTED, MediaError, MediaSource,
};

use super::battery::{BatteryError, BatterySource, NativeBatterySource, sort_batteries};
use super::config::{AudioAction, MediaAction, ProviderKind};
use super::data::{CpuData, DiskMountData, MemoryData, NetworkData};

/// Sampling failed. Logged by the sampler task; the tick is skipped.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ProviderError {
    #[error("sampling task did not complete: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("failed to serialize provider payload: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error(transparent)]
    Battery(#[from] BatteryError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error(transparent)]
    Audio(#[from] AudioError),
}

/// Filesystems that are not real storage and would clutter the bar.
const PSEUDO_FILESYSTEMS: &[&str] = &[
    "autofs",
    "devfs",
    "devtmpfs",
    "efivarfs",
    "fuse.portal",
    "overlay",
    "proc",
    "ramfs",
    "squashfs",
    "sysfs",
    "tmpfs",
];

/// Cumulative non-loopback interface totals; baseline for per-second rates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NetworkTotals {
    rx_total: u64,
    tx_total: u64,
    sampled_at: Instant,
}

/// The shared `sysinfo::System` plus the CPU refresh bookkeeping that must
/// stay under the same lock.
struct SystemState {
    system: System,
    last_cpu_refresh: Option<Instant>,
}

/// Shared `sysinfo` handles; all sampler tasks of one hub reuse these.
///
/// Every refresh is blocking I/O and therefore runs in
/// [`tokio::task::spawn_blocking`].
#[derive(Clone)]
pub(crate) struct SystemSampler {
    battery: Arc<dyn BatterySource>,
    audio: Arc<OnceLock<AudioSource>>,
    media: MediaSource,
    system: Arc<Mutex<SystemState>>,
    disks: Arc<Mutex<Disks>>,
    networks: Arc<Mutex<Networks>>,
}

impl SystemSampler {
    pub(crate) fn new() -> Self {
        Self::with_battery_source(Arc::new(NativeBatterySource))
    }

    fn with_battery_source(battery: Arc<dyn BatterySource>) -> Self {
        Self {
            battery,
            audio: Arc::new(OnceLock::new()),
            media: MediaSource::default(),
            system: Arc::new(Mutex::new(SystemState {
                system: System::new(),
                last_cpu_refresh: None,
            })),
            disks: Arc::new(Mutex::new(Disks::new_with_refreshed_list())),
            networks: Arc::new(Mutex::new(Networks::new_with_refreshed_list())),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_battery_source(battery: Arc<dyn BatterySource>) -> Self {
        Self::with_battery_source(battery)
    }

    /// Samples one payload for `kind`. `network_baseline` is the per-task
    /// delta state for [`ProviderKind::Network`]; unused by other kinds.
    pub(crate) async fn sample(
        &self,
        kind: ProviderKind,
        network_baseline: &mut Option<NetworkTotals>,
    ) -> Result<Value, ProviderError> {
        match kind {
            ProviderKind::Battery => self.sample_battery().await,
            ProviderKind::Cpu => self.sample_cpu().await,
            ProviderKind::Memory => self.sample_memory().await,
            ProviderKind::Disk => self.sample_disks().await,
            ProviderKind::Network => self.sample_network(network_baseline).await,
            ProviderKind::Media => self.sample_media().await,
            ProviderKind::Audio => self.sample_audio(),
        }
    }

    pub(crate) fn supports(&self, kind: ProviderKind) -> bool {
        match kind {
            ProviderKind::Audio => AUDIO_SUPPORTED,
            ProviderKind::Media => MEDIA_SUPPORTED,
            _ => true,
        }
    }

    pub(crate) async fn media_action(
        &self,
        session_id: Option<&str>,
        action: MediaAction,
    ) -> Result<(), MediaError> {
        self.media.action(session_id, action).await
    }

    pub(crate) async fn audio_action(&self, action: AudioAction) -> Result<(), AudioError> {
        let audio = self.audio.get_or_init(AudioSource::default).clone();
        tokio::task::spawn_blocking(move || audio.action(action))
            .await
            .map_err(|error| AudioError::Backend(format!("audio action task failed: {error}")))?
    }

    async fn sample_battery(&self) -> Result<Value, ProviderError> {
        let source = Arc::clone(&self.battery);
        let mut data = tokio::task::spawn_blocking(move || source.sample()).await??;
        sort_batteries(&mut data.batteries);
        Ok(serde_json::to_value(data)?)
    }

    async fn sample_media(&self) -> Result<Value, ProviderError> {
        Ok(serde_json::to_value(self.media.sample().await?)?)
    }

    fn sample_audio(&self) -> Result<Value, ProviderError> {
        let audio = self.audio.get_or_init(AudioSource::default);
        Ok(serde_json::to_value(audio.sample()?)?)
    }

    async fn sample_cpu(&self) -> Result<Value, ProviderError> {
        let state = Arc::clone(&self.system);
        let data = tokio::task::spawn_blocking(move || {
            let mut state = lock_unpoisoned(&state);
            // CPU usage is the diff between two refreshes; sysinfo needs at
            // least MINIMUM_CPU_UPDATE_INTERVAL between them.
            match state.last_cpu_refresh {
                Some(last) => {
                    let elapsed = last.elapsed();
                    if elapsed < MINIMUM_CPU_UPDATE_INTERVAL {
                        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL - elapsed);
                    }
                }
                None => {
                    // refresh_cpu_specifics does not populate the CPU list on
                    // a fresh System; refresh_cpu_list does.
                    state.system.refresh_cpu_list(CpuRefreshKind::everything());
                    std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
                }
            }
            state
                .system
                .refresh_cpu_specifics(CpuRefreshKind::everything());
            state.last_cpu_refresh = Some(Instant::now());

            let per_core: Vec<f32> = state
                .system
                .cpus()
                .iter()
                .map(|cpu| cpu.cpu_usage())
                .collect();
            let core_count = per_core.len();
            let frequency_mhz = state
                .system
                .cpus()
                .iter()
                .map(|cpu| cpu.frequency())
                .max()
                .unwrap_or(0);
            CpuData {
                usage_percent: state.system.global_cpu_usage(),
                per_core,
                core_count,
                frequency_mhz,
            }
        })
        .await?;
        Ok(serde_json::to_value(data)?)
    }

    async fn sample_memory(&self) -> Result<Value, ProviderError> {
        let state = Arc::clone(&self.system);
        let data = tokio::task::spawn_blocking(move || {
            let mut state = lock_unpoisoned(&state);
            state.system.refresh_memory();
            let used_bytes = state.system.used_memory();
            let total_bytes = state.system.total_memory();
            MemoryData {
                used_bytes,
                total_bytes,
                usage_percent: percent(used_bytes, total_bytes),
                swap_used_bytes: state.system.used_swap(),
                swap_total_bytes: state.system.total_swap(),
            }
        })
        .await?;
        Ok(serde_json::to_value(data)?)
    }

    async fn sample_disks(&self) -> Result<Value, ProviderError> {
        let disks = Arc::clone(&self.disks);
        let data = tokio::task::spawn_blocking(move || {
            let mut disks = lock_unpoisoned(&disks);
            disks.refresh(true);
            let mut mounts: Vec<DiskMountData> = disks
                .list()
                .iter()
                .filter(|disk| !is_pseudo_fs(disk))
                .map(|disk| {
                    let total_bytes = disk.total_space();
                    let used_bytes = total_bytes.saturating_sub(disk.available_space());
                    DiskMountData {
                        mount_point: disk.mount_point().to_string_lossy().into_owned(),
                        used_bytes,
                        total_bytes,
                        usage_percent: percent(used_bytes, total_bytes),
                    }
                })
                .collect();
            // Stable order keeps identical system states byte-identical, so
            // the hub's value dedupe works across refreshes.
            mounts.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
            mounts
        })
        .await?;
        Ok(serde_json::to_value(data)?)
    }

    async fn sample_network(
        &self,
        baseline: &mut Option<NetworkTotals>,
    ) -> Result<Value, ProviderError> {
        let networks = Arc::clone(&self.networks);
        let (rx_total, tx_total) = tokio::task::spawn_blocking(move || {
            let mut networks = lock_unpoisoned(&networks);
            networks.refresh(true);
            let mut rx_total: u64 = 0;
            let mut tx_total: u64 = 0;
            for (name, interface) in networks.list() {
                if is_loopback(name) {
                    continue;
                }
                rx_total = rx_total.saturating_add(interface.total_received());
                tx_total = tx_total.saturating_add(interface.total_transmitted());
            }
            (rx_total, tx_total)
        })
        .await?;

        let current = NetworkTotals {
            rx_total,
            tx_total,
            sampled_at: Instant::now(),
        };
        let data = match baseline.replace(current) {
            Some(previous) => rates(previous, current),
            None => NetworkData {
                rx_bytes_per_sec: 0,
                tx_bytes_per_sec: 0,
            },
        };
        Ok(serde_json::to_value(data)?)
    }
}

fn rates(previous: NetworkTotals, current: NetworkTotals) -> NetworkData {
    let secs = current
        .sampled_at
        .duration_since(previous.sampled_at)
        .as_secs_f64();
    if secs <= 0.0 {
        return NetworkData {
            rx_bytes_per_sec: 0,
            tx_bytes_per_sec: 0,
        };
    }
    NetworkData {
        rx_bytes_per_sec: per_second(current.rx_total.saturating_sub(previous.rx_total), secs),
        tx_bytes_per_sec: per_second(current.tx_total.saturating_sub(previous.tx_total), secs),
    }
}

fn per_second(delta_bytes: u64, secs: f64) -> u64 {
    (delta_bytes as f64 / secs).round() as u64
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
    }
}

fn is_pseudo_fs(disk: &sysinfo::Disk) -> bool {
    let file_system = disk.file_system().to_string_lossy().to_ascii_lowercase();
    disk.total_space() == 0 || PSEUDO_FILESYSTEMS.contains(&file_system.as_str())
}

fn is_loopback(interface_name: &str) -> bool {
    interface_name == "lo"
        || interface_name == "lo0"
        || interface_name.to_ascii_lowercase().contains("loopback")
}
