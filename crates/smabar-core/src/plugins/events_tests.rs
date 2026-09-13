use std::sync::Barrier;

use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;

use super::*;

fn render(plugin: &str, tile: &str, target: &str, html: &str) -> PluginEvent {
    PluginEvent::UiRender {
        plugin_id: plugin.to_string(),
        tile_id: tile.to_string(),
        target: target.to_string(),
        html: html.to_string(),
        ttl_ms: None,
    }
}

fn added(tiles: &[&str]) -> PluginEvent {
    PluginEvent::Added {
        plugin_id: "demo".to_string(),
        name: "Demo".to_string(),
        icon_data_url: None,
        tiles: tiles
            .iter()
            .map(|id| serde_json::from_value(json!({ "id": id, "name": id })).unwrap())
            .collect(),
        settings_schema: None,
    }
}

#[test]
fn received_one_shot_render_is_already_cached() {
    let events = PluginEvents::new(16);
    let mut receiver = events.subscribe();
    events
        .send(render("demo", "one", "flyout", "one shot"))
        .unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Ok(PluginEvent::UiRender { .. })
    ));
    let snapshot = events.current_ui();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].html, "one shot");
}

#[test]
fn an_earlier_one_shot_is_replayed_without_replaying_a_popup() {
    let events = PluginEvents::new(16);
    // No listeners yet: publication reports that fact, but persistent HTML
    // still belongs to the supervisor's snapshot.
    assert!(
        events
            .send(render("demo", "one", "flyout", "before"))
            .is_err()
    );
    assert!(
        events
            .send(render("demo", "one", "popup", "notice"))
            .is_err()
    );
    let (snapshot, mut receiver) = events.subscribe_with_ui();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].html, "before");
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

    events
        .send(render("demo", "one", "flyout", "after"))
        .unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Ok(PluginEvent::UiRender { html, .. }) if html == "after"
    ));
}

#[test]
fn a_render_racing_subscription_is_on_exactly_one_side_of_the_snapshot() {
    let events = PluginEvents::new(16);
    let ready = Barrier::new(3);
    // Both operations contend for the same boundary. No runtime scheduling
    // or sleeps decide whether an event tracker happened to catch up first.
    let cache = lock_unpoisoned(&events.ui);
    std::thread::scope(|scope| {
        let publication = scope.spawn(|| {
            ready.wait();
            let _ = events.send(render("demo", "one", "hover", "concurrent"));
        });
        let subscription = scope.spawn(|| {
            ready.wait();
            events.subscribe_with_ui()
        });
        ready.wait();
        drop(cache);
        publication.join().unwrap();
        let (snapshot, mut receiver) = subscription.join().unwrap();
        let cached = snapshot
            .iter()
            .filter(|row| row.html == "concurrent")
            .count();
        let streamed = match receiver.try_recv() {
            Ok(PluginEvent::UiRender { html, .. }) => {
                assert_eq!(html, "concurrent");
                1
            }
            Err(TryRecvError::Empty) => 0,
            other => panic!("unexpected stream result: {other:?}"),
        };
        assert_eq!(cached + streamed, 1);
    });
}

#[test]
fn lag_resync_keeps_the_retained_tail_and_continues_at_the_snapshot_boundary() {
    for lag_already_reported in [false, true] {
        let events = PluginEvents::new(4);
        let mut receiver = events.subscribe();
        for html in ["old", "new"] {
            events.send(render("demo", "one", "tile", html)).unwrap();
        }
        events.send(added(&["one"])).unwrap();
        events
            .send(render("demo", "one", "popup", "notice"))
            .unwrap();
        events
            .send(PluginEvent::Status {
                plugin_id: "demo".to_string(),
                status: crate::plugins::PluginStatus::Running,
                error: None,
            })
            .unwrap();
        events
            .send(render("demo", "one", "tile", "latest"))
            .unwrap();
        if lag_already_reported {
            assert!(matches!(receiver.try_recv(), Err(TryRecvError::Lagged(2))));
        }

        let (snapshot, pending) = events.resync_ui(&mut receiver);
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].target, "tile");
        assert_eq!(snapshot[0].html, "latest");
        assert!(matches!(
            pending.as_slice(),
            [
                PluginEvent::Added { plugin_id, tiles, .. },
                PluginEvent::UiRender { target, html: popup, .. },
                PluginEvent::Status {
                    status: crate::plugins::PluginStatus::Running,
                    error: None,
                    ..
                },
                PluginEvent::UiRender { html: latest, .. },
            ] if plugin_id == "demo" && tiles.len() == 1 && tiles[0].id == "one"
                && target == "popup" && popup == "notice" && latest == "latest"
        ));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

        events
            .send(render("demo", "one", "tile", "future"))
            .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(PluginEvent::UiRender { html, .. }) if html == "future"
        ));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }
}

#[test]
fn identical_renders_refresh_freshness_and_are_still_published() {
    let events = PluginEvents::new(16);
    let event = render("demo", "one", "tile", "same");
    {
        let mut cache = lock_unpoisoned(&events.ui);
        record_ui(&mut cache, &event, 10);
        record_ui(
            &mut cache,
            &render("demo", "detail", "flyout", "detail"),
            20,
        );
        record_ui(&mut cache, &render("other", "one", "tile", "other"), 20);
    }
    assert!(events.tiles_rendered_since("demo", 20).is_empty());
    record_ui(&mut lock_unpoisoned(&events.ui), &event, 20);
    assert_eq!(events.tiles_rendered_since("demo", 20), ["one"]);
    assert!(events.tiles_rendered_since("demo", 21).is_empty());

    let mut receiver = events.subscribe();
    events.send(event.clone()).unwrap();
    events.send(event).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            receiver.try_recv(),
            Ok(PluginEvent::UiRender { html, .. }) if html == "same"
        ));
    }
}

#[test]
fn a_restart_keeps_declared_tiles_and_removal_clears_only_its_plugin() {
    let events = PluginEvents::new(16);
    let _receiver = events.subscribe();
    events.send(added(&["one", "two"])).unwrap();
    events
        .send(render("demo", "one", "tile", "survivor"))
        .unwrap();
    events
        .send(render("demo", "one", "hover", "preview"))
        .unwrap();
    events
        .send(render("demo", "two", "flyout", "removed tile"))
        .unwrap();
    events
        .send(render("other", "one", "tile", "other plugin"))
        .unwrap();
    events.send(added(&["one"])).unwrap();
    let snapshot = events.current_ui();
    let rows: Vec<_> = snapshot
        .iter()
        .map(|row| {
            (
                row.plugin_id.as_str(),
                row.tile_id.as_str(),
                row.target.as_str(),
                row.html.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("demo", "one", "hover", "preview"),
            ("demo", "one", "tile", "survivor"),
            ("other", "one", "tile", "other plugin"),
        ]
    );

    events
        .send(PluginEvent::Removed {
            plugin_id: "demo".to_string(),
        })
        .unwrap();
    let snapshot = events.current_ui();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].plugin_id, "other");
}
