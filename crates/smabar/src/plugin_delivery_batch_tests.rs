use std::sync::{Arc, Barrier, Mutex};

use super::tests::{Emitted, record, render, request, targets};
use super::*;

#[test]
fn a_run_signals_each_surface_once_and_take_returns_the_last_html_per_key() {
    let delivery = PluginDelivery::default();
    let active = request(1, "demo", "one");
    let mut events = Vec::new();
    delivery.open(&active, true, record(&mut events)).unwrap();
    events.clear();
    let run = [
        render("demo", "one", "tile", "t1"),
        render("demo", "one", "hover", "h1"),
        render("demo", "one", "tile", "t2"),
        render("other", "two", "tile", "o1"),
        render("demo", "one", "flyout", "B"),
        render("other", "two", "flyout", "closed tile"),
    ];
    delivery
        .handle_many(&run, Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(
        events,
        vec![
            ("plugin-ui-bar".to_string(), json!({})),
            ("plugin-ui-overlay".to_string(), json!({ "generation": 1 })),
        ]
    );
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("hover", "h1"), ("tile", "t2"), ("tile", "o1")]
    );
    let overlay = delivery.take(SurfaceRole::Overlay, Some(&active));
    assert_eq!(targets(&overlay), [("flyout", "B"), ("hover", "h1")]);
    assert!(overlay.iter().all(|item| item["generation"] == 1));
    assert!(delivery.take(SurfaceRole::Bar, None).is_empty());
    events.clear();
    let settled = [
        render("demo", "one", "tile", "t2"),
        render("demo", "one", "hover", "h1"),
        render("other", "two", "tile", "o1"),
        render("demo", "one", "flyout", "B"),
    ];
    delivery
        .handle_many(&settled, Some(&active), record(&mut events))
        .unwrap();
    assert!(events.is_empty(), "a run without new HTML signals nothing");
}

#[test]
fn a_failed_signal_keeps_the_run_pending_for_the_next_one() {
    let delivery = PluginDelivery::default();
    let active = request(1, "demo", "one");
    let mut events = Vec::new();
    delivery.open(&active, true, record(&mut events)).unwrap();
    events.clear();
    let run = [
        render("demo", "one", "tile", "t"),
        render("demo", "one", "hover", "h"),
    ];
    let mut attempted = Vec::new();
    assert!(
        delivery
            .handle_many(&run, Some(&active), |channel, _| {
                attempted.push(channel.to_string());
                anyhow::bail!("bar signal failed")
            })
            .is_err()
    );
    assert_eq!(attempted, ["plugin-ui-bar"]);
    delivery
        .handle_many(&[], Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(events.len(), 2, "an empty run still signals pending HTML");
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("hover", "h"), ("tile", "t")]
    );
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Overlay, Some(&active))),
        [("hover", "h")]
    );
    assert!(delivery.take(SurfaceRole::Overlay, None).is_empty());
    assert!(delivery.take(SurfaceRole::Settings, None).is_empty());
}

#[test]
fn keys_removed_within_a_run_are_not_pending() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    let run = [
        render("demo", "one", "tile", "gone"),
        PluginEvent::Removed {
            plugin_id: "demo".to_string(),
        },
        render("other", "one", "tile", "o"),
    ];
    delivery
        .handle_many(&run, None, record(&mut events))
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("tile", "o")]
    );
    assert_eq!(delivery.bar_snapshot().len(), 1);
}

#[test]
fn an_opening_emit_finishes_before_a_concurrent_live_update() {
    let delivery = PluginDelivery::default();
    let active = request(1, "demo", "one");
    delivery
        .handle(&render("demo", "one", "flyout", "A"), None, |_, _| Ok(()))
        .unwrap();
    let opening = Barrier::new(2);
    let release = Barrier::new(2);
    let output = Arc::new(Mutex::new(Emitted::new()));
    std::thread::scope(|scope| {
        let opener = scope.spawn(|| {
            delivery
                .open(&active, true, |channel, payload| {
                    output.lock().unwrap().push((channel.into(), payload));
                    opening.wait();
                    release.wait();
                    Ok(())
                })
                .unwrap();
        });
        opening.wait();
        let update = scope.spawn(|| {
            delivery
                .handle(
                    &render("demo", "one", "flyout", "B"),
                    Some(&active),
                    |channel, payload| {
                        output.lock().unwrap().push((channel.into(), payload));
                        Ok(())
                    },
                )
                .unwrap();
        });
        release.wait();
        opener.join().unwrap();
        update.join().unwrap();
    });
    let events = output.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "surface-flyout");
    assert_eq!(events[0].1["content"]["flyout"], "A");
    assert_eq!(events[1].0, "plugin-ui-overlay");
    assert_eq!(events[1].1["generation"], 1);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Overlay, Some(&active))),
        [("flyout", "B")]
    );
}
