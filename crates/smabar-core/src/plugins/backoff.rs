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

/// Delay before the Nth automatic retry of a failed managed-runtime install
/// (1-based): 15, 30, 60, 120, 240 seconds, capped at 300. uv fails fast
/// offline, so this starts slower than [`restart_delay`] and never hammers
/// a dead network.
pub(crate) fn runtime_retry_delay(attempt: u32) -> Duration {
    const BASE_SECS: u64 = 15;
    const CAP_SECS: u64 = 300;
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_secs((BASE_SECS << exponent).min(CAP_SECS))
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

    #[test]
    fn runtime_retries_start_slow_and_cap_at_five_minutes() {
        let expected = [
            (1, 15),
            (2, 30),
            (3, 60),
            (4, 120),
            (5, 240),
            (6, 300),
            (7, 300),
        ];
        for (attempt, seconds) in expected {
            assert_eq!(
                runtime_retry_delay(attempt),
                Duration::from_secs(seconds),
                "attempt: {attempt}"
            );
        }
        assert_eq!(runtime_retry_delay(0), Duration::from_secs(15));
        assert_eq!(runtime_retry_delay(u32::MAX), Duration::from_secs(300));
    }
}
