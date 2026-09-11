use super::super::{BarPosition, LayoutConfig, SettingsWindowConfig};
use super::*;

fn layout_top() -> LayoutConfig {
    LayoutConfig {
        monitor: None,
        position: BarPosition::Top,
        ..LayoutConfig::default()
    }
}

fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    (dir, paths)
}

fn state_with_default() -> (WatchState, SmabarConfig) {
    let config = SmabarConfig::default();
    let json = config.to_pretty_json().expect("serialize");
    (WatchState::new(config.clone(), &json), config)
}

#[test]
fn observe_suppresses_unchanged_content() {
    let (mut state, config) = state_with_default();
    let json = config.to_pretty_json().expect("serialize");
    assert!(state.observe(&json).is_none());
}

#[test]
fn observe_detects_real_change_with_old_and_new() {
    let (mut state, old) = state_with_default();
    let new = SmabarConfig {
        layout: layout_top(),
        settings_window: SettingsWindowConfig {
            width: 900,
            ..SettingsWindowConfig::default()
        },
        ..old.clone()
    };
    let json = new.to_pretty_json().expect("serialize");

    let change = state.observe(&json).expect("change expected");
    assert_eq!(change.old, old);
    assert_eq!(change.new, new);
    assert!(change.layout_changed());
    assert!(!change.language_changed());
    assert!(!change.theme_changed());
    assert!(!change.shortcuts_changed());
    assert!(!change.plugins_hidden_changed());
    assert!(!change.effects_changed());
    assert!(change.settings_window_changed());
    assert!(state.observe(&json).is_none());
}

#[test]
fn observe_ignores_formatting_only_changes() {
    let (mut state, config) = state_with_default();
    let compact = serde_json::to_string(&config).expect("serialize");
    assert!(state.observe(&compact).is_none());
}

#[test]
fn observe_keeps_previous_config_on_invalid_json_then_recovers() {
    let (mut state, old) = state_with_default();
    assert!(state.observe("{ definitely broken").is_none());
    assert_eq!(state.current, old);

    let mut invalid = old.clone();
    invalid.mcp.port = 0;
    let json = serde_json::to_string(&invalid).expect("serialize invalid config");
    assert!(state.observe(&json).is_none());
    assert_eq!(state.current, old);

    let new = SmabarConfig {
        language: "de".to_string(),
        theme: "neon".to_string(),
        ..old
    };
    let json = new.to_pretty_json().expect("serialize");
    let change = state.observe(&json).expect("change after recovery");
    assert!(change.language_changed());
    assert!(change.theme_changed());
    assert_eq!(change.new, new);
}

#[test]
fn record_write_suppresses_the_echo_of_our_own_save() {
    let (mut state, _) = state_with_default();
    let saved = SmabarConfig {
        layout: layout_top(),
        language: "nl".to_string(),
        ..SmabarConfig::default()
    };
    let json = saved.to_pretty_json().expect("serialize");

    state.record_write(&json, saved.clone());
    assert!(state.observe(&json).is_none());
    assert_eq!(state.current, saved);
}

#[tokio::test]
async fn watcher_emits_external_edits_and_suppresses_own_saves() {
    const TEST_DEBOUNCE: Duration = Duration::from_millis(50);
    let (_dir, paths) = temp_paths();
    let watcher =
        ConfigWatcher::spawn_with_debounce(paths.clone(), TEST_DEBOUNCE).expect("spawn watcher");
    let mut rx = watcher.subscribe();
    assert_eq!(watcher.current(), SmabarConfig::default());

    let saved = SmabarConfig {
        layout: layout_top(),
        ..watcher.current()
    };
    watcher.save(&saved).expect("save");
    assert_eq!(watcher.current(), saved);
    tokio::time::sleep(TEST_DEBOUNCE * 6).await;

    let external = SmabarConfig {
        language: "de".to_string(),
        ..SmabarConfig::default()
    };
    let json = external.to_pretty_json().expect("serialize");
    std::fs::write(paths.config_file(), json).expect("external write");

    let change = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for config change")
        .expect("broadcast closed");
    assert_eq!(change.old, saved);
    assert_eq!(change.new, external);
    assert!(change.layout_changed());
    assert!(change.language_changed());
    assert_eq!(watcher.current(), external);
    assert!(matches!(
        rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn apply_broadcasts_exactly_one_change_and_skips_no_ops() {
    let (_dir, paths) = temp_paths();
    let watcher = ConfigWatcher::spawn(paths.clone()).expect("spawn watcher");
    let mut rx = watcher.subscribe();

    let old = watcher.current();
    let new = SmabarConfig {
        layout: layout_top(),
        ..old.clone()
    };
    watcher.apply(new.clone()).expect("apply");
    assert_eq!(watcher.current(), new);

    let change = rx.try_recv().expect("apply must broadcast the change");
    assert_eq!(change.old, old);
    assert_eq!(change.new, new);
    assert!(matches!(
        rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    watcher.apply(new.clone()).expect("apply unchanged");
    assert!(matches!(
        rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    let on_disk = std::fs::read_to_string(paths.config_file()).expect("read config");
    assert!(on_disk.contains("\"position\": \"top\""));
}

#[tokio::test]
async fn plugin_order_update_is_broadcast_and_survives_reload() {
    let (_dir, paths) = temp_paths();
    let watcher = ConfigWatcher::spawn(paths.clone()).expect("spawn watcher");
    let mut changes = watcher.subscribe();
    let order = vec!["plugin:weather:main", "plugin:clock:clock"];

    watcher
        .update(|current| {
            let (updated, _) = super::super::update::set_config_path_activating(
                &paths,
                current,
                "pluginOrder",
                serde_json::json!(order),
            )
            .expect("update tile order");
            (updated, ())
        })
        .expect("persist tile order");

    let change = changes.try_recv().expect("tile order change");
    assert_eq!(change.new.plugin_order, order);
    assert_eq!(watcher.current(), change.new);
    drop(watcher);

    let reloaded = ConfigWatcher::spawn(paths).expect("reload saved config");
    assert_eq!(reloaded.current(), change.new);
}

#[tokio::test(flavor = "multi_thread")]
async fn with_current_orders_a_side_effect_before_the_next_update() {
    let (_dir, paths) = temp_paths();
    let watcher = Arc::new(ConfigWatcher::spawn(paths).expect("spawn watcher"));
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let reading = Arc::clone(&watcher);
    let read = tokio::task::spawn_blocking(move || {
        reading.with_current(|current| {
            entered_tx.send(current.theme.clone()).expect("entered");
            release_rx.recv().expect("release");
        });
    });
    assert_eq!(entered_rx.recv().expect("read theme"), "default");

    let updating = Arc::clone(&watcher);
    let update = tokio::task::spawn_blocking(move || {
        updating
            .update(|current| {
                let mut next = current.clone();
                next.theme = "neon".to_string();
                (next, ())
            })
            .expect("update")
    });
    tokio::task::yield_now().await;
    assert!(!update.is_finished(), "update crossed the read lock");

    release_tx.send(()).expect("release read");
    read.await.expect("read task");
    update.await.expect("update task");
    assert_eq!(watcher.current().theme, "neon");
}

#[tokio::test]
async fn concurrent_relative_updates_preserve_both_changes() {
    let (_dir, paths) = temp_paths();
    let watcher = Arc::new(ConfigWatcher::spawn(paths).expect("spawn watcher"));
    let mut changes = watcher.subscribe();
    let start = Arc::new(std::sync::Barrier::new(3));
    let language = {
        let watcher = Arc::clone(&watcher);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            watcher.update(|current| {
                let mut updated = current.clone();
                updated.language = "de".to_string();
                (updated, ())
            })
        })
    };
    let export_dir = {
        let watcher = Arc::clone(&watcher);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            watcher.update(|current| {
                let mut updated = current.clone();
                updated.theme_export_dir = "/tmp/themes".to_string();
                (updated, ())
            })
        })
    };
    start.wait();
    language.join().expect("language thread").expect("update");
    export_dir.join().expect("export thread").expect("update");

    let current = watcher.current();
    assert_eq!(current.language, "de");
    assert_eq!(current.theme_export_dir, "/tmp/themes");

    let first = changes.try_recv().expect("first ordered change");
    let second = changes.try_recv().expect("second ordered change");
    assert_eq!(first.old, SmabarConfig::default());
    assert_eq!(first.new, second.old);
    assert_eq!(second.new, current);
}
