use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use serde_json::json;
use tokio::time::timeout;

use super::config::{MediaAction, ProviderConfig, ProviderKind};
use super::data::{
    AudioData, AudioOutput, CpuData, DiskMountData, MediaData, MemoryData, NetworkData,
    ProviderEvent,
};
use super::hub::ProviderHub;

const RECV_TIMEOUT: Duration = Duration::from_secs(30);

fn config(kind: ProviderKind, interval_ms: u64) -> ProviderConfig {
    ProviderConfig { kind, interval_ms }
}

#[test]
fn config_hash_matches_for_identical_configs() {
    let state = RandomState::new();
    let a = config(ProviderKind::Cpu, 2_000);
    let b = config(ProviderKind::Cpu, 2_000);
    assert_eq!(a, b);
    assert_eq!(state.hash_one(a), state.hash_one(b));
}

#[test]
fn config_hash_differs_for_different_configs() {
    let state = RandomState::new();
    let base = config(ProviderKind::Cpu, 2_000);
    assert_ne!(
        state.hash_one(base),
        state.hash_one(config(ProviderKind::Cpu, 5_000))
    );
    assert_ne!(
        state.hash_one(base),
        state.hash_one(config(ProviderKind::Memory, 2_000))
    );
}

#[test]
fn zero_interval_normalizes_to_kind_default() {
    assert_eq!(config(ProviderKind::Cpu, 0).normalized().interval_ms, 2_000);
    assert_eq!(
        config(ProviderKind::Memory, 0).normalized().interval_ms,
        2_000
    );
    assert_eq!(
        config(ProviderKind::Network, 0).normalized().interval_ms,
        2_000
    );
    assert_eq!(
        config(ProviderKind::Battery, 0).normalized().interval_ms,
        5_000
    );
    assert_eq!(
        config(ProviderKind::Media, 0).normalized().interval_ms,
        1_000
    );
    assert_eq!(config(ProviderKind::Audio, 0).normalized().interval_ms, 500);
    assert_eq!(
        config(ProviderKind::Disk, 0).normalized().interval_ms,
        30_000
    );
    // Explicit intervals stay untouched.
    assert_eq!(
        config(ProviderKind::Disk, 5_000).normalized().interval_ms,
        5_000
    );
    // Zero and the explicit default normalize to the same dedupe key.
    assert_eq!(
        config(ProviderKind::Cpu, 0).normalized(),
        config(ProviderKind::Cpu, 2_000)
    );
}

#[test]
fn event_key_is_stable_and_normalized() {
    assert_eq!(config(ProviderKind::Cpu, 2_000).key(), "cpu:2000");
    assert_eq!(config(ProviderKind::Disk, 0).key(), "disk:30000");
    assert_eq!(config(ProviderKind::Network, 500).key(), "network:500");
}

#[test]
fn provider_kind_serializes_as_camel_case_string() {
    assert_eq!(json!(ProviderKind::Battery), json!("battery"));
    assert_eq!(json!(ProviderKind::Cpu), json!("cpu"));
    assert_eq!(json!(ProviderKind::Memory), json!("memory"));
    assert_eq!(json!(ProviderKind::Disk), json!("disk"));
    assert_eq!(json!(ProviderKind::Network), json!("network"));
    assert_eq!(json!(ProviderKind::Media), json!("media"));
    assert_eq!(json!(ProviderKind::Audio), json!("audio"));
    let parsed: ProviderKind = serde_json::from_value(json!("network")).unwrap();
    assert_eq!(parsed, ProviderKind::Network);
}

#[test]
fn provider_kind_registry_drives_names_and_parsing() {
    assert_eq!(
        ProviderKind::ALL.map(ProviderKind::as_str),
        [
            "cpu", "memory", "disk", "network", "battery", "media", "audio"
        ]
    );
    for kind in ProviderKind::ALL {
        assert_eq!(ProviderKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(ProviderKind::parse("temperature"), None);
}

#[test]
fn media_is_advertised_exactly_when_the_platform_source_exists() {
    let names = ProviderHub::new().available_names();
    assert_eq!(names.contains(&"media"), crate::platform::MEDIA_SUPPORTED);
}

#[test]
fn audio_is_advertised_exactly_when_the_platform_source_exists() {
    let names = ProviderHub::new().available_names();
    assert_eq!(names.contains(&"audio"), crate::platform::AUDIO_SUPPORTED);
}

#[test]
fn media_actions_parse_and_serialize_as_the_rpc_allowlist() {
    for (name, action) in [
        ("play", MediaAction::Play),
        ("pause", MediaAction::Pause),
        ("playPause", MediaAction::PlayPause),
        ("next", MediaAction::Next),
        ("previous", MediaAction::Previous),
    ] {
        assert_eq!(MediaAction::parse(name), Some(action));
        assert_eq!(json!(action), json!(name));
    }
    assert_eq!(MediaAction::parse("stop"), None);
    assert_eq!(MediaAction::parse("Play"), None);
}

#[test]
fn media_payload_defaults_to_no_sessions() {
    assert_eq!(
        json!(MediaData::default()),
        json!({"currentSessionId": null, "sessions": []})
    );
}

#[test]
fn audio_payload_is_neutral_and_camel_case() {
    assert_eq!(json!(AudioData::default()), json!({"defaultOutput": null}));
    assert_eq!(
        json!(AudioData {
            default_output: Some(AudioOutput {
                name: "Speakers".into(),
                volume_percent: 37.5,
                muted: false,
            })
        }),
        json!({
            "defaultOutput": {
                "name": "Speakers",
                "volumePercent": 37.5,
                "muted": false
            }
        })
    );
}

#[test]
fn provider_config_uses_camel_case_fields() {
    let parsed: ProviderConfig =
        serde_json::from_value(json!({ "kind": "disk", "intervalMs": 0 })).unwrap();
    assert_eq!(parsed, config(ProviderKind::Disk, 0));
    assert_eq!(json!(parsed), json!({ "kind": "disk", "intervalMs": 0 }));
}

#[test]
fn payloads_serialize_with_camel_case_fields() {
    let cpu = json!(CpuData {
        usage_percent: 12.5,
        per_core: vec![10.0, 15.0],
        core_count: 2,
        frequency_mhz: 3_400,
    });
    assert_eq!(
        cpu,
        json!({
            "usagePercent": 12.5,
            "perCore": [10.0, 15.0],
            "coreCount": 2,
            "frequencyMhz": 3400,
        })
    );

    let memory = json!(MemoryData {
        used_bytes: 512,
        total_bytes: 1024,
        usage_percent: 50.0,
        swap_used_bytes: 0,
        swap_total_bytes: 2048,
    });
    assert_eq!(
        memory,
        json!({
            "usedBytes": 512,
            "totalBytes": 1024,
            "usagePercent": 50.0,
            "swapUsedBytes": 0,
            "swapTotalBytes": 2048,
        })
    );

    let mount = json!(DiskMountData {
        mount_point: "/".into(),
        used_bytes: 10,
        total_bytes: 100,
        usage_percent: 10.0,
    });
    assert_eq!(
        mount,
        json!({
            "mountPoint": "/",
            "usedBytes": 10,
            "totalBytes": 100,
            "usagePercent": 10.0,
        })
    );

    let network = json!(NetworkData {
        rx_bytes_per_sec: 1_000,
        tx_bytes_per_sec: 2_000,
    });
    assert_eq!(
        network,
        json!({ "rxBytesPerSec": 1000, "txBytesPerSec": 2000 })
    );

    let event = json!(ProviderEvent {
        key: "cpu:2000".into(),
        kind: ProviderKind::Cpu,
        data: json!({ "usagePercent": 1.0 }),
        ts_ms: 1_700_000_000_000,
    });
    assert_eq!(
        event,
        json!({
            "key": "cpu:2000",
            "kind": "cpu",
            "data": { "usagePercent": 1.0 },
            "tsMs": 1_700_000_000_000_u64,
        })
    );
}

#[tokio::test]
async fn identical_configs_share_one_sampler() {
    let hub = ProviderHub::new();
    let _a = hub.subscribe(config(ProviderKind::Memory, 60_000)).await;
    let _b = hub.subscribe(config(ProviderKind::Memory, 60_000)).await;
    assert_eq!(hub.active_sampler_count(), 1);

    // Zero-interval configs dedupe against their normalized form.
    let _c = hub.subscribe(config(ProviderKind::Memory, 0)).await;
    let _d = hub.subscribe(config(ProviderKind::Memory, 2_000)).await;
    assert_eq!(hub.active_sampler_count(), 2);

    // A different interval is a different sampler.
    let _e = hub.subscribe(config(ProviderKind::Memory, 30_000)).await;
    assert_eq!(hub.active_sampler_count(), 3);
}

#[tokio::test]
async fn cached_value_replays_to_new_subscriber() {
    let hub = ProviderHub::new();
    let cfg = config(ProviderKind::Memory, 60_000);
    let seeded = ProviderEvent {
        key: cfg.key(),
        kind: cfg.kind,
        data: json!({ "usedBytes": 42 }),
        ts_ms: 1,
    };
    hub.inject_cache(cfg, seeded.clone());

    let mut subscription = hub.subscribe(cfg).await;
    // The replay is stored in the subscription itself, so this resolves
    // immediately regardless of sampler timing.
    let event = timeout(RECV_TIMEOUT, subscription.recv())
        .await
        .expect("replay should resolve immediately")
        .expect("hub is alive");
    assert_eq!(event, seeded);
}

#[test]
fn snapshot_returns_all_cached_events_sorted_by_key() {
    let hub = ProviderHub::new();
    assert!(hub.snapshot().is_empty());

    let memory_cfg = config(ProviderKind::Memory, 60_000);
    let cpu_cfg = config(ProviderKind::Cpu, 2_000);
    let memory_event = ProviderEvent {
        key: memory_cfg.key(),
        kind: memory_cfg.kind,
        data: json!({ "usedBytes": 42 }),
        ts_ms: 2,
    };
    let cpu_event = ProviderEvent {
        key: cpu_cfg.key(),
        kind: cpu_cfg.kind,
        data: json!({ "usagePercent": 7.0 }),
        ts_ms: 1,
    };
    hub.inject_cache(memory_cfg, memory_event.clone());
    hub.inject_cache(cpu_cfg, cpu_event.clone());

    assert_eq!(hub.snapshot(), vec![cpu_event, memory_event]);
}

#[tokio::test]
async fn last_drop_stops_sampler_and_resubscribe_restarts() {
    let hub = ProviderHub::new();
    let cfg = config(ProviderKind::Memory, 60_000);
    let a = hub.subscribe(cfg).await;
    let b = hub.subscribe(cfg).await;
    assert_eq!(hub.active_sampler_count(), 1);

    drop(a);
    assert_eq!(hub.active_sampler_count(), 1);
    drop(b);
    assert_eq!(hub.active_sampler_count(), 0);

    let _c = hub.subscribe(cfg).await;
    assert_eq!(hub.active_sampler_count(), 1);
}

#[tokio::test]
async fn memory_smoke_first_emission_and_replay() {
    let hub = ProviderHub::new();
    let cfg = config(ProviderKind::Memory, 60_000);

    let mut first = hub.subscribe(cfg).await;
    let event = timeout(RECV_TIMEOUT, first.recv())
        .await
        .expect("first tick emits immediately")
        .expect("hub is alive");
    assert_eq!(event.key, "memory:60000");
    assert_eq!(event.kind, ProviderKind::Memory);
    let total = event.data["totalBytes"].as_u64().expect("totalBytes");
    let used = event.data["usedBytes"].as_u64().expect("usedBytes");
    let percent = event.data["usagePercent"].as_f64().expect("usagePercent");
    assert!(total > 0);
    assert!(used <= total);
    assert!((0.0..=100.0).contains(&percent));

    // A second subscriber gets exactly the cached emission replayed; the
    // 60 s interval guarantees no second tick interferes.
    let mut second = hub.subscribe(cfg).await;
    let replayed = timeout(RECV_TIMEOUT, second.recv())
        .await
        .expect("replay resolves immediately")
        .expect("hub is alive");
    assert_eq!(replayed, event);
}

#[tokio::test]
async fn cpu_smoke_values_are_plausible() {
    let hub = ProviderHub::new();
    let mut subscription = hub.subscribe(config(ProviderKind::Cpu, 60_000)).await;
    let event = timeout(RECV_TIMEOUT, subscription.recv())
        .await
        .expect("first tick emits after CPU priming")
        .expect("hub is alive");
    assert_eq!(subscription.key(), "cpu:60000");

    let usage = event.data["usagePercent"].as_f64().expect("usagePercent");
    assert!((0.0..=100.5).contains(&usage), "usage {usage} out of range");
    let core_count = event.data["coreCount"].as_u64().expect("coreCount");
    assert!(core_count >= 1);
    let per_core = event.data["perCore"].as_array().expect("perCore");
    assert_eq!(per_core.len() as u64, core_count);
}

#[tokio::test]
async fn disk_smoke_mounts_are_plausible() {
    let hub = ProviderHub::new();
    let mut subscription = hub.subscribe(config(ProviderKind::Disk, 0)).await;
    let event = timeout(RECV_TIMEOUT, subscription.recv())
        .await
        .expect("first tick emits immediately")
        .expect("hub is alive");
    let mounts = event.data.as_array().expect("disk payload is an array");
    for mount in mounts {
        assert!(mount["mountPoint"].as_str().is_some());
        let total = mount["totalBytes"].as_u64().expect("totalBytes");
        assert!(total > 0, "pseudo filesystems must be filtered out");
        let percent = mount["usagePercent"].as_f64().expect("usagePercent");
        assert!((0.0..=100.0).contains(&percent));
    }
}

#[tokio::test]
async fn network_smoke_first_emission_reports_zero_rates() {
    let hub = ProviderHub::new();
    let mut subscription = hub.subscribe(config(ProviderKind::Network, 60_000)).await;
    let event = timeout(RECV_TIMEOUT, subscription.recv())
        .await
        .expect("first tick emits immediately")
        .expect("hub is alive");
    // The first sample has no baseline for a delta, so rates are zero.
    assert_eq!(
        event.data,
        json!({ "rxBytesPerSec": 0, "txBytesPerSec": 0 })
    );
}
