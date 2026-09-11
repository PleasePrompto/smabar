//! Restart backoff policy — pure and unit-tested.

use std::time::Duration;

/// Consecutive-failure limit; reaching it parks the plugin as `failed` until
/// its folder changes again.
pub(crate) const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// A run at least this long counts as stable and resets the failure count.
pub(crate) const STABLE_RUN: Duration = Duration::from_secs(5 * 60);

/// Delay before restart attempt N (1-based): 1, 2, 4, 8, ... seconds,
/// capped at 60.
pub(crate) fn restart_delay(consecutive_failures: u32) -> Duration {
    const CAP_SECS: u64 = 60;
    let exponent = consecutive_failures.saturating_sub(1).min(6);
    Duration::from_secs((1u64 << exponent).min(CAP_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_from_one_second_and_caps_at_sixty() {
        let expected = [(1, 1), (2, 2), (3, 4), (4, 8), (5, 16), (6, 32), (7, 60)];
        for (failures, seconds) in expected {
            assert_eq!(
                restart_delay(failures),
                Duration::from_secs(seconds),
                "failures: {failures}"
            );
        }
    }

    #[test]
    fn delay_stays_capped_for_extreme_inputs() {
        assert_eq!(restart_delay(0), Duration::from_secs(1));
        assert_eq!(restart_delay(100), Duration::from_secs(60));
        assert_eq!(restart_delay(u32::MAX), Duration::from_secs(60));
    }
}
