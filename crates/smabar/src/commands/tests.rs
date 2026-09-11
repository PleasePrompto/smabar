use std::sync::{Arc, Mutex};

use super::*;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("capture lock")).into_owned()
    }
}

#[test]
fn shell_events_reach_the_core_log_with_their_level_and_target() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || writer.clone())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        log_shell_event(
            "error",
            "bridge init failed",
            json!({"where": "initBridge"}),
        );
        log_shell_event("warn", "a warning", Value::Null);
        log_shell_event("debug", "a detail", Value::Null);
        log_shell_event("shout", "still logged", Value::Null);
    });

    let text = captured.text();
    assert!(text.contains("smabar::shell"), "{text}");
    assert!(
        text.contains("ERROR") && text.contains("bridge init failed"),
        "{text}"
    );
    assert!(
        text.contains("initBridge"),
        "structured fields are kept: {text}"
    );
    assert!(
        text.contains("WARN") && text.contains("a warning"),
        "{text}"
    );
    assert!(
        text.contains("DEBUG") && text.contains("a detail"),
        "{text}"
    );
    assert!(
        text.contains("still logged"),
        "an unknown level falls back: {text}"
    );
}

#[test]
fn persistent_bar_level_yields_to_fullscreen() {
    use WindowLevel::{Bottom, Panel, Top};
    let level = effective_window_level;
    let mut config = SmabarConfig::default();
    assert_eq!(level(&config, false), Panel);
    assert_eq!(level(&config, true), Bottom);

    config.z_order = ZOrder::Bottom;
    assert_eq!(level(&config, false), Bottom);

    config.layout.behavior = LayoutBehavior::Float;
    config.z_order = ZOrder::Top;
    assert_eq!(level(&config, true), Top);

    config.layout.behavior = LayoutBehavior::Autohide;
    assert_eq!(level(&config, true), Top);

    config.layout.behavior = LayoutBehavior::Reserve;
    config.layout.yield_to_fullscreen = false;
    assert_eq!(level(&config, true), Top);
}
