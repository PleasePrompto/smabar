use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::time::timeout;

use super::battery::{BatteryError, BatterySource};
use super::config::{ProviderConfig, ProviderKind};
use super::data::{BatteryData, BatteryInfo, BatteryState};
use super::hub::ProviderHub;
use super::sampler::SystemSampler;

struct FakeBatterySource {
    samples: Mutex<VecDeque<BatteryData>>,
}

impl FakeBatterySource {
    fn new(samples: Vec<BatteryData>) -> Self {
        Self {
            samples: Mutex::new(samples.into()),
        }
    }
}

impl BatterySource for FakeBatterySource {
    fn sample(&self) -> Result<BatteryData, BatteryError> {
        let mut samples = self.samples.lock().unwrap();
        let sample = if samples.len() > 1 {
            samples.pop_front()
        } else {
            samples.front().cloned()
        };
        Ok(sample.unwrap_or(BatteryData { batteries: vec![] }))
    }
}

fn battery(charge_percent: f32, state: BatteryState) -> BatteryInfo {
    BatteryInfo {
        charge_percent,
        health_percent: 88.0,
        cycle_count: None,
        state,
        is_charging: state == BatteryState::Charging,
        time_till_empty: None,
        time_till_full: Some(3_600_000.0),
        power_consumption: 18.0,
        voltage: 11.9,
    }
}

#[test]
fn battery_payload_serializes_with_empty_list_as_absent_state() {
    let present = battery(73.5, BatteryState::Discharging);
    assert_eq!(
        json!(BatteryData {
            batteries: vec![BatteryInfo {
                health_percent: 91.0,
                cycle_count: Some(120),
                time_till_empty: Some(7_200_000.0),
                time_till_full: None,
                power_consumption: 8.5,
                voltage: 12.25,
                ..present
            }],
        }),
        json!({
            "batteries": [{
                "chargePercent": 73.5,
                "healthPercent": 91.0,
                "cycleCount": 120,
                "state": "discharging",
                "isCharging": false,
                "timeTillEmpty": 7200000.0,
                "timeTillFull": null,
                "powerConsumption": 8.5,
                "voltage": 12.25,
            }],
        })
    );
    assert_eq!(
        json!(BatteryData { batteries: vec![] }),
        json!({ "batteries": [] })
    );
}

#[tokio::test]
async fn battery_emits_dynamic_absent_and_present_states_from_fake_source() {
    let present = battery(42.0, BatteryState::Charging);
    let source = Arc::new(FakeBatterySource::new(vec![
        BatteryData { batteries: vec![] },
        BatteryData {
            batteries: vec![present.clone()],
        },
    ]));
    let sampler = SystemSampler::with_test_battery_source(source);
    let hub = ProviderHub::with_test_sampler(sampler);
    let mut subscription = hub
        .subscribe(ProviderConfig {
            kind: ProviderKind::Battery,
            interval_ms: 1,
        })
        .await;

    let absent = timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("fake battery sample should be immediate")
        .expect("hub is alive");
    assert_eq!(absent.data, json!({ "batteries": [] }));

    let detected = timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("changed fake battery sample should emit")
        .expect("hub is alive");
    assert_eq!(detected.data, json!({ "batteries": [present] }));
}

#[tokio::test]
async fn battery_order_is_canonical_when_the_backend_reorders_devices() {
    let lower = battery(28.0, BatteryState::Discharging);
    let higher = battery(76.0, BatteryState::Discharging);
    let source = Arc::new(FakeBatterySource::new(vec![
        BatteryData {
            batteries: vec![higher.clone(), lower.clone()],
        },
        BatteryData {
            batteries: vec![lower.clone(), higher.clone()],
        },
    ]));
    let sampler = SystemSampler::with_test_battery_source(source);
    let hub = ProviderHub::with_test_sampler(sampler);
    let mut subscription = hub
        .subscribe(ProviderConfig {
            kind: ProviderKind::Battery,
            interval_ms: 1,
        })
        .await;

    let first = timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("first battery sample should be immediate")
        .expect("hub is alive");
    assert_eq!(
        first.data,
        json!({ "batteries": [lower, higher] }),
        "backend enumeration order must not choose the displayed battery"
    );

    assert!(
        timeout(Duration::from_millis(50), subscription.recv())
            .await
            .is_err(),
        "reordering an unchanged battery set must be deduplicated"
    );
}
