//! Process-only ablation for memory investigations; never persisted in config.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

const WARMUP: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Off,
    Observe,
    NoEvents,
    NoState,
    NoDom,
    NoClocks,
}

impl Mode {
    pub(crate) fn parse(value: Option<&str>) -> anyhow::Result<Self> {
        match value {
            None => Ok(Self::Off),
            Some("observe") => Ok(Self::Observe),
            Some("no-events") => Ok(Self::NoEvents),
            Some("no-state") => Ok(Self::NoState),
            Some("no-dom") => Ok(Self::NoDom),
            Some("no-clocks") => Ok(Self::NoClocks),
            Some(_) => anyhow::bail!(
                "SMABAR_MEMORY_PROBE must be observe, no-events, no-state, no-dom or no-clocks; unset it for normal operation"
            ),
        }
    }

    pub(crate) fn name(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Observe => Some("observe"),
            Self::NoEvents => Some("no-events"),
            Self::NoState => Some("no-state"),
            Self::NoDom => Some("no-dom"),
            Self::NoClocks => Some("no-clocks"),
        }
    }

    fn suppresses(self, target: &str) -> bool {
        self == Self::NoEvents && target != "popup"
    }
}

pub(crate) struct Counters {
    mode: Mode,
    started: Instant,
    since: Instant,
    renders: u64,
    html_bytes: u64,
    suppressed: u64,
    sources: BTreeMap<(String, String, String), RenderCounts>,
}

#[derive(Default)]
struct RenderCounts {
    renders: u64,
    html_bytes: u64,
    suppressed: u64,
}

/// Correlates native flyout transitions with shell render/cleanup records.
/// Only identifiers and phases are recorded, never plugin HTML or field values.
pub(crate) fn surface(app: &AppHandle, phase: &'static str, generation: u64, tile: Option<&str>) {
    if let Some(probe) = app.try_state::<Mode>()
        && let Some(mode) = probe.name()
    {
        tracing::info!(
            app_pid = std::process::id(),
            mode,
            phase,
            generation,
            tile,
            "memory probe flyout"
        );
    }
}

impl Counters {
    pub(crate) fn new(mode: Mode) -> Self {
        Self {
            mode,
            started: Instant::now(),
            since: Instant::now(),
            renders: 0,
            html_bytes: 0,
            suppressed: 0,
            sources: BTreeMap::new(),
        }
    }

    /// The core still receives every render. Only persistent shell delivery
    /// is cut after 30 seconds of startup; snapshot replay and managed popups
    /// remain available. Slow initial renders after warmup are also suppressed.
    pub(crate) fn suppress(
        &mut self,
        plugin_id: &str,
        tile_id: &str,
        target: &str,
        html_bytes: usize,
    ) -> bool {
        if self.mode == Mode::Off || target == "popup" {
            return false;
        }
        let suppressed = self.mode.suppresses(target) && self.started.elapsed() >= WARMUP;
        self.renders += 1;
        self.html_bytes += html_bytes as u64;
        self.suppressed += u64::from(suppressed);
        let source = self
            .sources
            .entry((plugin_id.to_owned(), tile_id.to_owned(), target.to_owned()))
            .or_default();
        source.renders += 1;
        source.html_bytes += html_bytes as u64;
        source.suppressed += u64::from(suppressed);
        if self.since.elapsed() >= Duration::from_secs(30) {
            tracing::info!(
                app_pid = std::process::id(),
                mode = self.mode.name(),
                elapsed_ms = self.since.elapsed().as_millis() as u64,
                renders = self.renders,
                html_bytes = self.html_bytes,
                suppressed = self.suppressed,
                "memory probe persistent plugin delivery"
            );
            for ((plugin_id, tile_id, target), source) in &self.sources {
                tracing::info!(
                    app_pid = std::process::id(),
                    mode = self.mode.name(),
                    elapsed_ms = self.since.elapsed().as_millis() as u64,
                    plugin_id,
                    tile_id,
                    target,
                    renders = source.renders,
                    html_bytes = source.html_bytes,
                    suppressed = source.suppressed,
                    "memory probe plugin source"
                );
            }
            self.sources.clear();
            self.since = Instant::now();
            self.renders = 0;
            self.html_bytes = 0;
            self.suppressed = 0;
        }
        suppressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_transport_probe_suppresses_persistent_renders() {
        assert_eq!(Mode::parse(None).unwrap(), Mode::Off);
        for mode in [
            Mode::Observe,
            Mode::NoEvents,
            Mode::NoState,
            Mode::NoDom,
            Mode::NoClocks,
        ] {
            assert_eq!(Mode::parse(mode.name()).unwrap(), mode);
            let mut counters = Counters::new(mode);
            assert!(!counters.suppress("clock", "clock", "tile", 1));
            counters.started -= WARMUP;
            assert!(!counters.suppress("clock", "clock", "popup", 10));
            for target in ["tile", "hover", "flyout"] {
                assert_eq!(
                    counters.suppress("clock", "clock", target, 20),
                    mode == Mode::NoEvents
                );
            }
            assert_eq!(counters.renders, 4);
            assert_eq!(counters.html_bytes, 61);
        }
        let mut normal = Counters::new(Mode::Off);
        assert!(!normal.suppress("clock", "clock", "tile", 10));
        assert_eq!(normal.renders, 0);
        assert!(normal.sources.is_empty());
        assert!(Mode::parse(Some("")).is_err());
        assert!(Mode::parse(Some("no-eveents")).is_err());
    }

    #[test]
    fn source_samples_separate_plugins_and_targets_then_release_the_interval() {
        let mut counters = Counters::new(Mode::Observe);
        for (plugin, target, bytes) in [
            ("clock", "tile", 5),
            ("clock", "tile", 7),
            ("clock", "flyout", 13),
            ("weather", "tile", 17),
        ] {
            assert!(!counters.suppress(plugin, "main", target, bytes));
        }
        assert_eq!(counters.sources.len(), 3);
        let clock = &counters.sources[&("clock".into(), "main".into(), "tile".into())];
        assert_eq!(
            (clock.renders, clock.html_bytes, clock.suppressed),
            (2, 12, 0)
        );
        counters.since -= Duration::from_secs(31);
        assert!(!counters.suppress("weather", "main", "tile", 19));
        assert!(counters.sources.is_empty());
        assert_eq!(
            (counters.renders, counters.html_bytes, counters.suppressed),
            (0, 0, 0)
        );
    }
}
