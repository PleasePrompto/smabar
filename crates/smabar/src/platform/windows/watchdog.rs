//! Pure liveness and retry decisions for the Windows mouse-hook watchdog.

pub(super) const POLL_INTERVAL_MS: u32 = 2_000;
const INITIAL_RETRY_TICKS: u8 = 2;
const MAX_RETRY_TICKS: u8 = 30;
const REPORT_COOLDOWN_TICKS: u16 = (5 * 60_000 / POLL_INTERVAL_MS) as u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Observation {
    pub input_tick: u32,
    pub cursor: Point,
    pub hook_tick: u32,
    pub hook_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    None,
    Reinstall { report_loss: bool },
    Recovered { report_recovery: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HookMark {
    tick: u32,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Healthy {
        suspect: Option<HookMark>,
    },
    Attempting {
        retry_ticks: u8,
    },
    Lost {
        installed: Option<HookMark>,
        missed_after_install: bool,
        retry_ticks: u8,
        next_retry_ticks: u8,
    },
}

pub(super) struct Watchdog {
    previous: Observation,
    phase: Phase,
    report_cooldown_ticks: u16,
    incident_reported: bool,
}

impl Watchdog {
    pub(super) fn new(initial: Observation) -> Self {
        Self {
            previous: initial,
            phase: Phase::Healthy { suspect: None },
            report_cooldown_ticks: 0,
            incident_reported: false,
        }
    }

    pub(super) fn observe(&mut self, current: Observation) -> Action {
        self.report_cooldown_ticks = self.report_cooldown_ticks.saturating_sub(1);
        let action = match self.phase {
            Phase::Healthy { suspect } => self.observe_healthy(current, suspect),
            Phase::Attempting { .. } => Action::None,
            Phase::Lost {
                installed,
                mut missed_after_install,
                mut retry_ticks,
                next_retry_ticks,
            } => {
                if installed.is_some_and(|mark| mark != hook_mark(current)) {
                    self.phase = Phase::Healthy { suspect: None };
                    let report_recovery = std::mem::take(&mut self.incident_reported);
                    Action::Recovered { report_recovery }
                } else {
                    missed_after_install |= mouse_activity_since(self.previous, current);
                    if (installed.is_none() || missed_after_install) && retry_ticks <= 1 {
                        self.phase = Phase::Attempting {
                            retry_ticks: next_retry_ticks,
                        };
                        Action::Reinstall { report_loss: false }
                    } else {
                        if installed.is_none() || missed_after_install {
                            retry_ticks = retry_ticks.saturating_sub(1);
                        }
                        self.phase = Phase::Lost {
                            installed,
                            missed_after_install,
                            retry_ticks,
                            next_retry_ticks,
                        };
                        Action::None
                    }
                }
            }
        };
        self.previous = current;
        action
    }

    pub(super) fn finish_attempt(&mut self, installed: bool, current: Observation) -> u32 {
        let Phase::Attempting { retry_ticks, .. } = self.phase else {
            return 0;
        };
        self.phase = Phase::Lost {
            installed: installed.then(|| hook_mark(current)),
            missed_after_install: !installed,
            retry_ticks,
            next_retry_ticks: retry_ticks.saturating_mul(2).min(MAX_RETRY_TICKS),
        };
        self.previous = current;
        u32::from(retry_ticks) * POLL_INTERVAL_MS
    }

    fn observe_healthy(&mut self, current: Observation, suspect: Option<HookMark>) -> Action {
        let current_hook = hook_mark(current);
        if suspect == Some(current_hook) {
            let report_loss = self.report_cooldown_ticks == 0;
            if report_loss {
                self.report_cooldown_ticks = REPORT_COOLDOWN_TICKS;
            }
            self.incident_reported = report_loss;
            self.phase = Phase::Attempting {
                retry_ticks: INITIAL_RETRY_TICKS,
            };
            return Action::Reinstall { report_loss };
        }
        self.phase = Phase::Healthy {
            suspect: mouse_activity_since(self.previous, current).then_some(current_hook),
        };
        Action::None
    }
}

fn hook_mark(observation: Observation) -> HookMark {
    HookMark {
        tick: observation.hook_tick,
        generation: observation.hook_generation,
    }
}

fn mouse_activity_since(previous: Observation, current: Observation) -> bool {
    current.input_tick != previous.input_tick
        && current.cursor != previous.cursor
        && hook_mark(current) == hook_mark(previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(input_tick: u32, x: i32, hook_tick: u32, hook_generation: u64) -> Observation {
        Observation {
            input_tick,
            cursor: Point { x, y: 20 },
            hook_tick,
            hook_generation,
        }
    }

    #[test]
    fn keyboard_activity_never_accuses_the_mouse_hook() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));

        assert_eq!(watchdog.observe(observation(20, 10, 10, 1)), Action::None);
        assert_eq!(watchdog.observe(observation(30, 10, 10, 1)), Action::None);
    }

    #[test]
    fn healthy_mouse_callbacks_accept_a_wrapped_input_tick() {
        let mut watchdog = Watchdog::new(observation(u32::MAX, 10, u32::MAX, 1));

        assert_eq!(watchdog.observe(observation(2, 30, 2, 2)), Action::None);
        assert_eq!(watchdog.observe(observation(3, 40, 3, 3)), Action::None);
    }

    #[test]
    fn missed_mouse_activity_requires_one_grace_poll_and_reports_once() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));

        assert_eq!(watchdog.observe(observation(20, 30, 10, 1)), Action::None);
        assert_eq!(
            watchdog.observe(observation(20, 30, 10, 1)),
            Action::Reinstall { report_loss: true }
        );
        assert_eq!(watchdog.observe(observation(20, 30, 10, 1)), Action::None);
    }

    #[test]
    fn a_hook_callback_cancels_a_suspected_loss() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));

        assert_eq!(watchdog.observe(observation(20, 30, 10, 1)), Action::None);
        assert_eq!(watchdog.observe(observation(20, 30, 20, 2)), Action::None);
        assert_eq!(watchdog.observe(observation(21, 30, 20, 2)), Action::None);
    }

    #[test]
    fn retry_backoff_doubles_without_repeating_the_loss_report() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));
        let missed = observation(20, 30, 10, 1);
        assert_eq!(watchdog.observe(missed), Action::None);
        assert_eq!(
            watchdog.observe(missed),
            Action::Reinstall { report_loss: true }
        );

        assert_eq!(watchdog.finish_attempt(false, missed), 4_000);
        assert_eq!(watchdog.observe(missed), Action::None);
        assert_eq!(
            watchdog.observe(missed),
            Action::Reinstall { report_loss: false }
        );
        assert_eq!(watchdog.finish_attempt(false, missed), 8_000);
        for _ in 0..3 {
            assert_eq!(watchdog.observe(missed), Action::None);
        }
        assert_eq!(
            watchdog.observe(missed),
            Action::Reinstall { report_loss: false }
        );
    }

    #[test]
    fn retry_backoff_stops_at_sixty_seconds() {
        let current = observation(20, 30, 10, 1);
        let mut watchdog = Watchdog::new(current);

        watchdog.phase = Phase::Attempting {
            retry_ticks: MAX_RETRY_TICKS,
        };
        assert_eq!(watchdog.finish_attempt(false, current), 60_000);
        for _ in 0..29 {
            assert_eq!(watchdog.observe(current), Action::None);
        }
        assert_eq!(
            watchdog.observe(current),
            Action::Reinstall { report_loss: false }
        );
        assert_eq!(watchdog.finish_attempt(false, current), 60_000);
    }

    #[test]
    fn a_reinstalled_hook_is_recovered_only_after_its_callback_runs() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));
        let missed = observation(20, 30, 10, 1);
        assert_eq!(watchdog.observe(missed), Action::None);
        assert_eq!(
            watchdog.observe(missed),
            Action::Reinstall { report_loss: true }
        );
        assert_eq!(watchdog.finish_attempt(true, missed), 4_000);

        assert_eq!(watchdog.observe(missed), Action::None);
        assert_eq!(
            watchdog.observe(observation(30, 40, 30, 2)),
            Action::Recovered {
                report_recovery: true
            }
        );
        assert_eq!(watchdog.observe(observation(30, 40, 30, 2)), Action::None);
    }

    #[test]
    fn a_flapping_hook_does_not_repeat_loss_or_recovery_logs() {
        let mut watchdog = Watchdog::new(observation(10, 10, 10, 1));
        let first_miss = observation(20, 30, 10, 1);
        assert_eq!(watchdog.observe(first_miss), Action::None);
        assert_eq!(
            watchdog.observe(first_miss),
            Action::Reinstall { report_loss: true }
        );
        assert_eq!(watchdog.finish_attempt(true, first_miss), 4_000);
        let first_callback = observation(30, 40, 30, 2);
        assert_eq!(
            watchdog.observe(first_callback),
            Action::Recovered {
                report_recovery: true
            }
        );

        let second_miss = observation(40, 50, 30, 2);
        assert_eq!(watchdog.observe(second_miss), Action::None);
        assert_eq!(
            watchdog.observe(second_miss),
            Action::Reinstall { report_loss: false }
        );
        assert_eq!(watchdog.finish_attempt(true, second_miss), 4_000);
        assert_eq!(
            watchdog.observe(observation(50, 60, 50, 3)),
            Action::Recovered {
                report_recovery: false
            }
        );
    }
}
