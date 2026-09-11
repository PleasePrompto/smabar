use starship_battery::units::electric_potential::volt;
use starship_battery::units::power::watt;
use starship_battery::units::ratio::percent;
use starship_battery::units::time::millisecond;
use starship_battery::{Battery, Manager, State};

use super::data::{BatteryData, BatteryInfo, BatteryState};

/// Battery discovery and sampling seam. Tests substitute a deterministic
/// source; production uses the platform-neutral `starship-battery` API.
pub(crate) trait BatterySource: Send + Sync {
    fn sample(&self) -> Result<BatteryData, BatteryError>;
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read battery information: {0}")]
pub(crate) struct BatteryError(#[from] starship_battery::Error);

pub(crate) struct NativeBatterySource;

impl BatterySource for NativeBatterySource {
    fn sample(&self) -> Result<BatteryData, BatteryError> {
        let manager = Manager::new()?;
        let batteries = manager
            .batteries()?
            .map(|battery| battery.map(|battery| to_info(&battery)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BatteryData { batteries })
    }
}

fn to_info(battery: &Battery) -> BatteryInfo {
    let state = battery.state();
    BatteryInfo {
        charge_percent: battery.state_of_charge().get::<percent>(),
        health_percent: battery.state_of_health().get::<percent>(),
        cycle_count: battery.cycle_count(),
        state: match state {
            State::Charging => BatteryState::Charging,
            State::Discharging => BatteryState::Discharging,
            State::Full => BatteryState::Full,
            State::Empty => BatteryState::Empty,
            State::Unknown => BatteryState::Unknown,
        },
        is_charging: state == State::Charging,
        time_till_empty: battery
            .time_to_empty()
            .map(|time| time.get::<millisecond>()),
        time_till_full: battery.time_to_full().map(|time| time.get::<millisecond>()),
        power_consumption: battery.energy_rate().get::<watt>(),
        voltage: battery.voltage().get::<volt>(),
    }
}

/// Canonical order independent of the platform iterator, whose API explicitly
/// makes no ordering guarantee. Lowest charge first also gives the Base Plugin
/// a deterministic, conservative battery to summarize.
pub(crate) fn sort_batteries(batteries: &mut [BatteryInfo]) {
    batteries.sort_by(|left, right| {
        left.charge_percent
            .total_cmp(&right.charge_percent)
            .then_with(|| left.health_percent.total_cmp(&right.health_percent))
            .then_with(|| left.cycle_count.cmp(&right.cycle_count))
            .then_with(|| left.state.cmp(&right.state))
            .then_with(|| left.is_charging.cmp(&right.is_charging))
            .then_with(|| optional_f32(left.time_till_empty, right.time_till_empty))
            .then_with(|| optional_f32(left.time_till_full, right.time_till_full))
            .then_with(|| left.power_consumption.total_cmp(&right.power_consumption))
            .then_with(|| left.voltage.total_cmp(&right.voltage))
    });
}

fn optional_f32(left: Option<f32>, right: Option<f32>) -> std::cmp::Ordering {
    match (left, right) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => left.total_cmp(&right),
    }
}
