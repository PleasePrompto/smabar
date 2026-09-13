use std::sync::{Arc, Barrier, Mutex};

use super::tests::{Emitted, record, render, request};
use super::*;

fn targets(payload: &Value) -> Vec<(&str, &str)> {
    payload
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["target"].as_str().unwrap(),
                item["html"].as_str().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_run_becomes_one_array_per_surface_with_the_last_html_per_key() {
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
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "plugin-ui-bar");
    assert_eq!(
        targets(&events[0].1),
        [("tile", "t2"), ("hover", "h1"), ("tile", "o1")]
    );
    assert_eq!(events[1].0, "plugin-ui-overlay");
    assert_eq!(targets(&events[1].1), [("hover", "h1"), ("flyout", "B")]);
    assert!(
        events[1]
            .1
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["generation"] == 1)
    );
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
    assert!(events.is_empty(), "a run without new HTML emits nothing");
}

#[test]
fn a_failed_bar_emit_leaves_the_run_pending_and_skips_the_overlay() {
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
                anyhow::bail!("bar emit failed")
            })
            .is_err()
    );
    assert_eq!(attempted, ["plugin-ui-bar"]);
    delivery
        .handle_many(&run, Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(targets(&events[0].1), [("tile", "t"), ("hover", "h")]);
    assert_eq!(targets(&events[1].1), [("hover", "h")]);
    events.clear();
    let changed = [render("demo", "one", "hover", "h2")];
    assert!(
        delivery
            .handle_many(&changed, Some(&active), |channel, _| {
                if channel == "plugin-ui-overlay" {
                    anyhow::bail!("overlay emit failed");
                }
                Ok(())
            })
            .is_err()
    );
    delivery
        .handle_many(&changed, Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(events.len(), 1, "the bar already received this hover");
    assert_eq!(events[0].0, "plugin-ui-overlay");
    assert_eq!(targets(&events[0].1), [("hover", "h2")]);
}

#[test]
fn keys_removed_within_a_run_are_not_published() {
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
    assert_eq!(targets(&events[0].1), [("tile", "o")]);
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
    assert_eq!(events[1].1[0]["html"], "B");
}
