use super::*;

#[test]
fn render_targets_are_the_four_current_surfaces() {
    for target in ["tile", "flyout", "hover", "popup"] {
        assert!(is_render_target(target), "target: {target}");
    }
    for target in ["", "toast"] {
        assert!(!is_render_target(target), "target: {target}");
    }
}

#[test]
fn optional_ttl_is_a_u32() {
    assert_eq!(optional_u32(&json!({}), "ttlMs"), Ok(None));
    assert_eq!(optional_u32(&json!({"ttlMs": null}), "ttlMs"), Ok(None));
    assert_eq!(
        optional_u32(&json!({"ttlMs": 6_000}), "ttlMs"),
        Ok(Some(6_000))
    );
    assert!(optional_u32(&json!({"ttlMs": -1}), "ttlMs").is_err());
    assert!(optional_u32(&json!({"ttlMs": u64::from(u32::MAX) + 1}), "ttlMs").is_err());
}

#[test]
fn a_misspelled_ttl_never_turns_a_popup_sticky() {
    for alias in ["ttlms", "ttl_ms", "ttl-ms", "TTLMS"] {
        assert_eq!(
            render_ttl(&json!({(alias): 9_000})),
            (Some(5_000), Some(INVALID_TTL_WARNING)),
            "alias: {alias}"
        );
    }
    assert_eq!(
        render_ttl(&json!({"ttlMs": 9_000, "ttlMS": 1_000})),
        (Some(9_000), Some(TTL_ALIAS_WARNING))
    );
    assert_eq!(
        render_ttl(&json!({"ttlMs": null, "TTLMS": 1_000})),
        (None, Some(TTL_ALIAS_WARNING))
    );
}

#[test]
fn provider_action_parser_accepts_media_with_an_optional_session() {
    let request = parse_provider_action_request(&json!({
        "kind": "media",
        "action": "playPause",
        "sessionId": "org.mpris.MediaPlayer2.spotify.instance_1"
    }))
    .expect("an extra field must not block the action");
    let ProviderActionRequest::Media(request) = request else {
        panic!("expected media action");
    };
    assert_eq!(request.action, MediaAction::PlayPause);
    assert_eq!(
        request.session_id.as_deref(),
        Some("org.mpris.MediaPlayer2.spotify.instance_1")
    );

    let request = parse_provider_action_request(&json!({"kind": "media", "action": "next"}))
        .expect("session id is optional");
    let ProviderActionRequest::Media(request) = request else {
        panic!("expected media action");
    };
    assert_eq!(request.session_id, None);
}

#[test]
fn provider_action_parser_accepts_audio_actions() {
    for (params, expected) in [
        (
            json!({"kind": "audio", "action": "setVolume", "volumePercent": 42}),
            AudioAction::SetVolume(42),
        ),
        (
            json!({"kind": "audio", "action": "setMuted", "muted": true}),
            AudioAction::SetMuted(true),
        ),
    ] {
        let request = parse_provider_action_request(&params).expect("valid audio action");
        let ProviderActionRequest::Audio(action) = request else {
            panic!("expected audio action");
        };
        assert_eq!(action, expected);
    }
}

#[test]
fn provider_action_parser_rejects_invalid_values_and_ambiguous_extras() {
    for params in [
        json!({"kind": "media", "action": "Quit"}),
        json!({"kind": "cpu", "action": "play"}),
        json!({"kind": "media", "action": "play", "sessionId": ""}),
        json!({"kind": "media", "action": "play", "sessionId": "x".repeat(513)}),
        json!({"kind": "media", "action": "play", "sessionId": 7}),
        json!({"kind": "audio", "action": "setVolume", "volumePercent": -1}),
        json!({"kind": "audio", "action": "setVolume", "volumePercent": 101}),
        json!({"kind": "audio", "action": "setVolume", "volumePercent": 42.5}),
        json!({"kind": "audio", "action": "setMuted", "muted": "true"}),
        json!({"kind": "audio", "action": "toggleMuted"}),
        json!({"kind": "media", "action": "play", "sessionid": "player"}),
        json!({"kind": "media", "action": "play", "futureOption": true}),
        json!({"kind": "audio", "action": "setMuted", "muted": true, "sessionId": "player"}),
        json!({"kind": "audio", "action": "setVolume", "volumePercent": 42, "future": true}),
    ] {
        assert!(
            parse_provider_action_request(&params).is_err(),
            "accepted {params}"
        );
    }

    let neutral = parse_media_action_request(&json!({
        "kind": "media",
        "action": "play",
        "sessionId": "platform-neutral-session-id"
    }))
    .expect("the plugin boundary must not impose MPRIS ids on future platforms");
    assert_eq!(
        neutral.session_id.as_deref(),
        Some("platform-neutral-session-id")
    );
}

#[test]
fn provider_intervals_are_bounded_and_typed() {
    for value in [
        json!({}),
        json!({"intervalMs": 0}),
        json!({"intervalMs": 250}),
        json!({"intervalMs": 3_600_000}),
    ] {
        assert!(provider_interval(&value).is_ok(), "rejected {value}");
    }
    for value in [
        json!({"intervalMs": 249}),
        json!({"intervalMs": 3_600_001}),
        json!({"intervalMs": null}),
        json!({"intervalMs": "500"}),
    ] {
        assert!(provider_interval(&value).is_err(), "accepted {value}");
    }
}
