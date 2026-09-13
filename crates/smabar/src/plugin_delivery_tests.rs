use super::*;

pub(super) type Emitted = Vec<(String, Value)>;

impl PluginDelivery {
    pub(super) fn handle(
        &self,
        event: &PluginEvent,
        active: Option<&OverlayFlyoutRequest>,
        emit: impl FnMut(&str, Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.handle_many(std::slice::from_ref(event), active, emit)
    }
}

pub(super) fn record(events: &mut Emitted) -> impl FnMut(&str, Value) -> anyhow::Result<()> + '_ {
    |channel, payload| {
        events.push((channel.to_string(), payload));
        Ok(())
    }
}

pub(super) fn render(plugin: &str, tile: &str, target: &str, html: &str) -> PluginEvent {
    PluginEvent::UiRender {
        plugin_id: plugin.to_string(),
        tile_id: tile.to_string(),
        target: target.to_string(),
        html: html.to_string(),
        ttl_ms: None,
    }
}

pub(super) fn request(generation: u64, plugin: &str, tile: &str) -> OverlayFlyoutRequest {
    OverlayFlyoutRequest {
        generation,
        tile_id: format!("plugin:{plugin}:{tile}"),
        mode: serde_json::from_value(json!("peek")).unwrap(),
        preserve_content: false,
    }
}

fn snapshot(plugin: &str, tile: &str, target: &str, html: &str) -> UiSnapshot {
    UiSnapshot {
        plugin_id: plugin.to_string(),
        tile_id: tile.to_string(),
        target: target.to_string(),
        html: html.to_string(),
    }
}

pub(super) fn targets(payload: &[Value]) -> Vec<(&str, &str)> {
    payload
        .iter()
        .map(|item| {
            (
                item["target"].as_str().unwrap(),
                item["html"].as_str().unwrap(),
            )
        })
        .collect()
}

fn channels(events: &Emitted) -> Vec<&str> {
    events.iter().map(|(channel, _)| channel.as_str()).collect()
}

#[test]
fn closed_updates_open_at_the_latest_one_shot_without_another_render() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    for html in ["A", "B", "C"] {
        delivery
            .handle(
                &render("demo", "one", "flyout", html),
                None,
                record(&mut events),
            )
            .unwrap();
    }
    assert!(events.is_empty());
    delivery
        .open(&request(1, "demo", "one"), true, record(&mut events))
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "surface-flyout");
    assert_eq!(
        events[0].1["content"],
        json!({ "hover": null, "flyout": "C" })
    );
}

#[test]
fn live_delivery_requires_the_opened_generation_and_matching_tile() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    let active = request(1, "demo", "one");
    let first = render("demo", "one", "flyout", "A");
    delivery
        .handle(&first, Some(&active), record(&mut events))
        .unwrap();
    assert!(
        events.is_empty(),
        "staging has not delivered its opening yet"
    );
    delivery.open(&active, true, record(&mut events)).unwrap();
    events.clear();
    delivery
        .handle(&first, Some(&active), record(&mut events))
        .unwrap();
    delivery
        .handle(
            &render("other", "one", "flyout", "other plugin"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    delivery
        .handle(
            &render("demo", "two", "flyout", "other tile"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert!(events.is_empty());
    assert!(
        delivery
            .take(SurfaceRole::Overlay, Some(&active))
            .is_empty()
    );
    delivery
        .handle(
            &render("demo", "one", "flyout", "B"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert_eq!(
        events,
        vec![("plugin-ui-overlay".to_string(), json!({ "generation": 1 }))]
    );
    let taken = delivery.take(SurfaceRole::Overlay, Some(&active));
    assert_eq!(
        taken,
        vec![
            json!({ "pluginId": "demo", "tileId": "one", "target": "flyout", "html": "B", "generation": 1 })
        ]
    );
    assert!(
        delivery
            .take(SurfaceRole::Overlay, Some(&active))
            .is_empty()
    );
    events.clear();

    let next = request(2, "demo", "one");
    delivery
        .handle(
            &render("demo", "one", "flyout", "C"),
            Some(&next),
            record(&mut events),
        )
        .unwrap();
    assert!(
        events.is_empty(),
        "a generation that has not opened is silent"
    );
    assert!(delivery.take(SurfaceRole::Overlay, Some(&next)).is_empty());
    delivery.open(&next, true, record(&mut events)).unwrap();
    assert_eq!(events[0].1["content"]["flyout"], "C");
    events.clear();
    delivery
        .handle(
            &render("demo", "one", "hover", "preview"),
            Some(&next),
            record(&mut events),
        )
        .unwrap();
    assert_eq!(channels(&events), ["plugin-ui-bar", "plugin-ui-overlay"]);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("hover", "preview")]
    );
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Overlay, Some(&next))),
        [("hover", "preview")]
    );
    events.clear();
    delivery
        .handle(
            &render("demo", "one", "tile", "bar"),
            Some(&next),
            record(&mut events),
        )
        .unwrap();
    assert_eq!(channels(&events), ["plugin-ui-bar"]);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("tile", "bar")]
    );
}

#[test]
fn reopening_refreshes_content_and_absent_targets_are_explicit_nulls() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    let active = request(1, "demo", "one");
    delivery
        .handle(
            &render("demo", "one", "flyout", "A"),
            None,
            record(&mut events),
        )
        .unwrap();
    delivery.open(&active, true, record(&mut events)).unwrap();
    events.clear();
    delivery
        .handle(
            &render("demo", "one", "flyout", "B"),
            None,
            record(&mut events),
        )
        .unwrap();
    assert!(events.is_empty());
    delivery
        .open(&request(2, "demo", "one"), true, record(&mut events))
        .unwrap();
    assert_eq!(
        events[0].1["content"],
        json!({ "hover": null, "flyout": "B" })
    );
    delivery
        .open(&request(3, "demo", "missing"), true, record(&mut events))
        .unwrap();
    assert_eq!(
        events[1].1["content"],
        json!({ "hover": null, "flyout": null })
    );
    delivery.reset(vec![snapshot("demo", "one", "hover", "")]);
    delivery
        .open(&request(4, "demo", "one"), true, record(&mut events))
        .unwrap();
    assert_eq!(
        events[2].1["content"],
        json!({ "hover": "", "flyout": null })
    );
}

#[test]
fn pinning_without_content_keeps_the_existing_delivery_state() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    let mut active = request(1, "demo", "one");
    let first = render("demo", "one", "flyout", "A");
    delivery.handle(&first, None, record(&mut events)).unwrap();
    delivery.open(&active, true, record(&mut events)).unwrap();
    events.clear();
    active.mode = serde_json::from_value(json!("pinned")).unwrap();
    active.preserve_content = true;
    delivery.open(&active, false, record(&mut events)).unwrap();
    assert_eq!(events[0].1["mode"], "pinned");
    assert_eq!(events[0].1["preserveContent"], true);
    assert!(events[0].1.get("content").is_none());
    events.clear();
    delivery
        .handle(&first, Some(&active), record(&mut events))
        .unwrap();
    assert!(
        events.is_empty(),
        "pin did not invalidate identical live HTML"
    );
}

#[test]
fn failures_keep_the_latest_html_pending_until_it_is_pulled() {
    let delivery = PluginDelivery::default();
    let active = request(1, "demo", "one");
    let mut events = Vec::new();
    delivery
        .handle(
            &render("demo", "one", "flyout", "A"),
            None,
            record(&mut events),
        )
        .unwrap();
    assert!(
        delivery
            .open(&active, true, |_, _| anyhow::bail!("emit failed"))
            .is_err()
    );
    delivery
        .handle(
            &render("demo", "one", "flyout", "B"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert!(
        events.is_empty(),
        "failed open must not unlock live delivery"
    );
    delivery.open(&active, true, record(&mut events)).unwrap();
    assert_eq!(events[0].1["content"]["flyout"], "B");
    events.clear();
    let changed = render("demo", "one", "flyout", "C");
    assert!(
        delivery
            .handle(&changed, Some(&active), |_, _| anyhow::bail!("emit failed"))
            .is_err()
    );
    delivery
        .handle(&changed, Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(channels(&events), ["plugin-ui-overlay"]);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Overlay, Some(&active))),
        [("flyout", "C")]
    );
    events.clear();
    let tile = render("demo", "one", "tile", "bar");
    assert!(
        delivery
            .handle(&tile, None, |_, _| anyhow::bail!("emit failed"))
            .is_err()
    );
    assert_eq!(delivery.bar_snapshot()[0]["html"], "bar");
    delivery.handle(&tile, None, record(&mut events)).unwrap();
    assert!(events.is_empty(), "the snapshot delivered this tile");
    delivery.invalidate();
    delivery.handle(&tile, None, record(&mut events)).unwrap();
    assert_eq!(channels(&events), ["plugin-ui-bar"]);
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("tile", "bar")]
    );
    events.clear();
    let hover = render("demo", "one", "hover", "preview");
    assert!(
        delivery
            .handle(&hover, Some(&active), |channel, _| {
                if channel == "plugin-ui-overlay" {
                    anyhow::bail!("overlay emit failed");
                }
                Ok(())
            })
            .is_err()
    );
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Bar, None)),
        [("hover", "preview")]
    );
    delivery
        .handle(&hover, Some(&active), record(&mut events))
        .unwrap();
    assert_eq!(
        channels(&events),
        ["plugin-ui-overlay"],
        "the bar already pulled this hover"
    );
    assert_eq!(
        targets(&delivery.take(SurfaceRole::Overlay, Some(&active))),
        [("flyout", "C"), ("hover", "preview")],
        "invalidate re-pends the pulled flyout as well"
    );
}

#[test]
fn reset_and_replay_restore_bar_and_active_one_shot_details() {
    let delivery = PluginDelivery::default();
    let active = request(1, "demo", "one");
    let mut events = Vec::new();
    delivery.reset(vec![
        snapshot("demo", "one", "tile", "bar"),
        snapshot("demo", "one", "hover", "preview"),
        snapshot("demo", "one", "flyout", "full"),
        snapshot("demo", "one", "popup", "transient"),
        snapshot("demo", "one", "unknown", "invalid"),
    ]);
    let bar = delivery.bar_snapshot();
    assert_eq!(bar.len(), 2);
    assert_eq!(bar[0]["target"], "hover");
    assert_eq!(bar[1]["target"], "tile");
    assert!(delivery.take(SurfaceRole::Bar, None).is_empty());
    delivery.replay(Some(&active), record(&mut events)).unwrap();
    assert_eq!(channels(&events), ["plugin-ui-bar", "surface-flyout"]);
    assert_eq!(
        events[1].1["content"],
        json!({ "hover": "preview", "flyout": "full" })
    );
    assert_eq!(delivery.take(SurfaceRole::Bar, None).len(), 2);
    events.clear();
    delivery
        .handle(
            &render("demo", "one", "flyout", "full"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert!(events.is_empty());
    delivery.reset(vec![snapshot("demo", "one", "flyout", "new")]);
    assert!(delivery.bar_snapshot().is_empty());
    delivery
        .handle(
            &render("demo", "one", "flyout", "new"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert!(events.is_empty());
    delivery.replay(Some(&active), record(&mut events)).unwrap();
    assert_eq!(channels(&events), ["surface-flyout"]);
    assert_eq!(
        events[0].1["content"],
        json!({ "hover": null, "flyout": "new" })
    );
    events.clear();
    delivery
        .handle(
            &render("demo", "one", "popup", "popup"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    delivery
        .handle(
            &render("demo", "one", "unknown", "unknown"),
            Some(&active),
            record(&mut events),
        )
        .unwrap();
    assert!(events.is_empty());
}

#[test]
fn lifecycle_keeps_surviving_content_and_invalidates_only_its_plugin() {
    let delivery = PluginDelivery::default();
    let mut events = Vec::new();
    for (plugin, tile, target, html) in [
        ("demo", "one", "tile", "bar"),
        ("demo", "one", "flyout", "surviving detail"),
        ("demo", "two", "flyout", "removed tile"),
        ("other", "one", "tile", "other"),
    ] {
        delivery
            .handle(
                &render(plugin, tile, target, html),
                None,
                record(&mut events),
            )
            .unwrap();
    }
    assert_eq!(delivery.take(SurfaceRole::Bar, None).len(), 2);
    events.clear();
    let added = PluginEvent::Added {
        plugin_id: "demo".to_string(),
        name: "Demo".to_string(),
        icon_data_url: None,
        tiles: vec![serde_json::from_value(json!({"id":"one", "name":"One"})).unwrap()],
        settings_schema: None,
    };
    delivery.handle(&added, None, record(&mut events)).unwrap();
    assert_eq!(channels(&events), ["plugin-ui-bar"]);
    let taken = delivery.take(SurfaceRole::Bar, None);
    assert_eq!(taken.len(), 1, "only the re-added plugin is pending again");
    assert_eq!(taken[0]["pluginId"], "demo");
    delivery
        .open(&request(1, "demo", "two"), true, record(&mut events))
        .unwrap();
    assert!(events[1].1["content"]["flyout"].is_null());
    delivery
        .open(&request(2, "demo", "one"), true, record(&mut events))
        .unwrap();
    assert_eq!(events[2].1["content"]["flyout"], "surviving detail");
    delivery
        .handle(
            &PluginEvent::Removed {
                plugin_id: "demo".to_string(),
            },
            None,
            record(&mut events),
        )
        .unwrap();
    let bar = delivery.bar_snapshot();
    assert_eq!(bar.len(), 1);
    assert_eq!(bar[0]["pluginId"], "other");
}
