//! Tests for manifest parsing and validation.

use super::*;

fn load_json(json: &str) -> Result<PluginManifest, ManifestError> {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join(MANIFEST_FILE), json).expect("write manifest");
    PluginManifest::load(dir.path())
}

const VALID_PYTHON: &str = r#"{
    "id": "clock-2",
    "name": "Clock",
    "version": "1.0.0",
    "description": "Shows the time",
    "protocolVersion": 1,
    "runtime": "python",
    "entry": "main.py",
    "tiles": [
        {"id": "clock", "name": "Clock", "hasFlyout": true}
    ],
    "settingsSchema": {"type": "object"}
}"#;

#[test]
fn valid_python_manifest_parses_all_fields() {
    let manifest = load_json(VALID_PYTHON).expect("valid manifest");
    assert_eq!(manifest.id, "clock-2");
    assert_eq!(manifest.name, "Clock");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.description.as_deref(), Some("Shows the time"));
    assert_eq!(manifest.protocol_version, 1);
    assert_eq!(manifest.runtime, PluginRuntime::Python);
    assert_eq!(manifest.entry.as_deref(), Some("main.py"));
    assert_eq!(manifest.tiles.len(), 1);
    assert!(!manifest.tiles[0].use_plugin_icon);
    let tile = &manifest.tiles[0];
    assert!(tile.has_flyout);
    assert!(manifest.settings_schema.is_some());
}

#[test]
fn valid_exec_manifest_defaults_optional_tile_fields() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "0.1.0", "protocolVersion": 1,
            "runtime": "exec", "command": ["python3", "main.py"],
            "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect("valid manifest");
    assert_eq!(manifest.runtime, PluginRuntime::Exec);
    assert_eq!(manifest.command, vec!["python3", "main.py"]);
    assert!(!manifest.tiles[0].has_flyout);
    assert!(manifest.description.is_none());
    assert!(manifest.settings_schema.is_none());
}

#[test]
fn unknown_tile_fields_do_not_reject_the_manifest() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "slots": [{"id": "action"}]}],
            "futureRoot": true}"#,
    )
    .expect("an unknown tile field must not stop the plugin");
    assert_eq!(manifest.tiles[0].id, "w");
    let diagnostic = manifest.diagnostic().expect("unknown fields are reported");
    assert!(diagnostic.contains("futureRoot"), "{diagnostic}");
    assert!(diagnostic.contains("slots"), "{diagnostic}");
}

/// Listed plugins carry the store's metadata block; the runtime must accept
/// it silently instead of warning about an unknown field on every start.
#[test]
fn the_store_block_is_a_known_root_field() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W"}],
            "store": {"keywords": ["demo"], "os": ["linux"], "minSmabar": "0.1.0"}}"#,
    )
    .expect("a store block must not stop the plugin");
    assert!(
        manifest.diagnostic().is_none(),
        "{:?}",
        manifest.diagnostic()
    );
}

#[test]
fn missing_file_is_a_read_error() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let err = PluginManifest::load(dir.path()).expect_err("missing manifest");
    assert!(matches!(err, ManifestError::Read { .. }));
    assert!(err.to_string().contains(MANIFEST_FILE));
}

#[test]
fn broken_json_and_missing_required_fields_are_parse_errors() {
    assert!(matches!(
        load_json("{ nope").expect_err("broken JSON"),
        ManifestError::Parse { .. }
    ));
    // "tiles" missing entirely.
    let err = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"]}"#,
    )
    .expect_err("missing tiles");
    assert!(matches!(err, ManifestError::Parse { .. }));
}

#[test]
fn invalid_ids_are_rejected() {
    for id in ["", "Has-Upper", "under_score", "dot.ted", "spa ce"] {
        let json = format!(
            r#"{{"id": "{id}", "name": "X", "version": "1", "protocolVersion": 1,
                "runtime": "exec", "command": ["a"],
                "tiles": [{{"id": "w", "name": "W"}}]}}"#
        );
        let err = load_json(&json).expect_err("invalid id");
        assert!(err.to_string().contains("[a-z0-9-]"), "id {id:?}: {err}");
    }
}

#[test]
fn wrong_protocol_version_is_rejected() {
    let err = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 2,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect_err("wrong protocol version");
    assert!(err.to_string().contains("protocolVersion 2"));
}

#[test]
fn empty_tiles_are_rejected() {
    let err = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"], "tiles": []}"#,
    )
    .expect_err("empty tiles");
    assert!(err.to_string().contains("at least one tile"));
}

#[test]
fn tile_branding_roundtrips_with_camel_case_keys() {
    let manifest = load_json(
        r##"{"id": "dhl", "name": "DHL", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "accent": "#FFCC00",
                         "accent2": "#D40511", "accentFg": "#1a1a1a"}]}"##,
    )
    .expect("valid branded manifest");
    let tile = &manifest.tiles[0];
    assert_eq!(tile.accent.as_deref(), Some("#FFCC00"));
    assert_eq!(tile.accent_2.as_deref(), Some("#D40511"));
    assert_eq!(tile.accent_fg.as_deref(), Some("#1a1a1a"));

    // Serialization (plugin-added event, MCP plugin_list) uses the same
    // camelCase keys the manifest uses — and omits unset options.
    let json = serde_json::to_value(tile).expect("serialize tile");
    assert_eq!(json["accent"], "#FFCC00");
    assert_eq!(json["accent2"], "#D40511");
    assert_eq!(json["accentFg"], "#1a1a1a");

    let plain = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect("valid manifest");
    let json = serde_json::to_value(&plain.tiles[0]).expect("serialize tile");
    for key in ["accent", "accent2", "accentFg", "iconSvg"] {
        assert!(json.get(key).is_none(), "unset {key} must be omitted");
    }
}

#[test]
fn tile_folder_icon_is_opt_in_and_validated() {
    for (input, enabled, warning) in [
        ("true", true, false),
        ("false", false, false),
        ("null", false, false),
        ("\"true\"", false, true),
        ("1", false, true),
    ] {
        let mut raw: Value = serde_json::from_str(VALID_PYTHON).expect("valid fixture");
        raw["tiles"][0]["usePluginIcon"] = serde_json::from_str(input).expect("test value");
        let manifest = load_json(&raw.to_string()).expect("optional icon does not stop plugin");
        assert_eq!(manifest.tiles[0].use_plugin_icon, enabled, "{input}");
        let serialized = serde_json::to_value(&manifest.tiles[0]).expect("serialize tile");
        assert_eq!(serialized["usePluginIcon"], enabled, "{input}");
        assert_eq!(manifest.diagnostic().is_some(), warning, "{input}");
        if warning {
            let diagnostic = manifest.diagnostic().expect("invalid choice is reported");
            assert!(diagnostic.contains("usePluginIcon"), "{diagnostic}");
            assert!(diagnostic.contains("true or false"), "{diagnostic}");
        }
    }
}

#[test]
fn tile_custom_icon_roundtrips_and_is_bounded() {
    let manifest = load_json(
        r##"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W",
                         "iconSvg": "<svg viewBox=\"0 0 24 24\"><path d=\"M2 2h20v20H2z\"/></svg>"}]}"##,
    )
    .expect("valid custom icon");
    let tile = &manifest.tiles[0];
    assert_eq!(
        tile.icon_svg.as_deref(),
        Some("<svg viewBox=\"0 0 24 24\"><path d=\"M2 2h20v20H2z\"/></svg>")
    );
    let json = serde_json::to_value(tile).expect("serialize tile");
    assert_eq!(
        json["iconSvg"],
        tile.icon_svg.as_deref().unwrap_or_default()
    );

    let oversized = "x".repeat(8 * 1024 + 1);
    let json = format!(
        r#"{{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{{"id": "w", "name": "W", "iconSvg": "{oversized}"}}]}}"#,
    );
    let manifest = load_json(&json).expect("only the oversized icon is ignored");
    assert!(manifest.tiles[0].icon_svg.is_none());
    let diagnostic = manifest.diagnostic().expect("oversized icon is reported");
    assert!(diagnostic.contains("iconSvg"), "{diagnostic}");
    assert!(diagnostic.contains("8192"), "{diagnostic}");
}

/// The manifests we SHIP have to parse and validate — a typo in one of
/// them would otherwise only surface on a user's first run, as a plugin
/// that silently never starts.
#[test]
fn every_bundled_plugin_manifest_is_valid() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .canonicalize()
        .expect("bundled plugins directory");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("read plugins dir") {
        let plugin = entry.expect("read entry").path();
        if !plugin.is_dir() {
            continue;
        }
        let manifest_path = plugin.join(MANIFEST_FILE);
        let raw = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("{}: {error}", manifest_path.display()));
        let manifest = PluginManifest::parse(&raw, &manifest_path)
            .unwrap_or_else(|error| panic!("{}: {error}", manifest_path.display()));
        // The folder name IS the plugin id; the supervisor relies on it.
        let folder = plugin.file_name().and_then(|name| name.to_str());
        assert_eq!(Some(manifest.id.as_str()), folder, "id must match folder");
        // A python plugin without its entry file never starts.
        if let Some(entry_file) = &manifest.entry {
            assert!(
                plugin.join(entry_file).is_file(),
                "{}: entry {entry_file} is missing",
                manifest.id
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 4,
        "expected the bundled plugins, found {checked}"
    );
}

#[test]
fn tile_tile_scale_is_optional_and_validated() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "tileScale": "l"}]}"#,
    )
    .expect("valid tile scale");
    assert_eq!(manifest.tiles[0].tile_scale, Some(TileScale::L));
    let json = serde_json::to_value(&manifest.tiles[0]).expect("serialize tile");
    assert_eq!(json["tileScale"], "l");

    let plain = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect("valid tile without scale");
    assert_eq!(plain.tiles[0].tile_scale, None);
    let json = serde_json::to_value(&plain.tiles[0]).expect("serialize tile");
    assert!(json.get("tileScale").is_none());

    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "tileScale": "huge"}]}"#,
    )
    .expect("only the unsupported scale is ignored");
    assert_eq!(manifest.tiles[0].tile_scale, None);
    assert!(
        manifest
            .diagnostic()
            .is_some_and(|message| message.contains("tileScale"))
    );
}

#[test]
fn tile_tile_chrome_is_optional_and_validated() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "tile": "flat"}]}"#,
    )
    .expect("valid flat tile override");
    assert_eq!(manifest.tiles[0].tile, Some(TileChrome::Flat));
    let json = serde_json::to_value(&manifest.tiles[0]).expect("serialize tile");
    assert_eq!(json["tile"], "flat");

    let plain = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect("valid tile without override");
    assert_eq!(plain.tiles[0].tile, None);
    let json = serde_json::to_value(&plain.tiles[0]).expect("serialize tile");
    assert!(json.get("tile").is_none());

    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{"id": "w", "name": "W", "tile": "glow"}]}"#,
    )
    .expect("only the unsupported tile chrome is ignored");
    assert_eq!(manifest.tiles[0].tile, None);
    assert!(
        manifest
            .diagnostic()
            .is_some_and(|message| message.contains(".tile"))
    );
}

#[test]
fn invalid_tile_branding_values_are_ignored() {
    for (field, value) in [
        ("accent", "red; display: none"),
        ("accent2", "url(javascript:alert(1))"),
        ("accentFg", "red } body {"),
    ] {
        let json = format!(
            r#"{{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
                "runtime": "exec", "command": ["a"],
                "tiles": [{{"id": "w", "name": "W", "{field}": "{value}"}}]}}"#
        );
        let manifest = load_json(&json).expect("invalid optional branding is ignored");
        let tile = &manifest.tiles[0];
        let value = match field {
            "accent" => &tile.accent,
            "accent2" => &tile.accent_2,
            _ => &tile.accent_fg,
        };
        assert!(value.is_none(), "{field} was retained");
        let diagnostic = manifest.diagnostic().expect("branding issue is reported");
        assert!(diagnostic.contains(field), "{field}: {diagnostic}");
        assert!(diagnostic.contains("CSS color"), "{field}: {diagnostic}");
    }

    // Over-long values are rejected even when structurally valid.
    let long = "a".repeat(129);
    let json = format!(
        r#"{{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": ["a"],
            "tiles": [{{"id": "w", "name": "W", "accent": "{long}"}}]}}"#
    );
    let manifest = load_json(&json).expect("over-long optional accent is ignored");
    assert!(manifest.tiles[0].accent.is_none());
    assert!(
        manifest
            .diagnostic()
            .is_some_and(|message| message.contains("128"))
    );
}

#[test]
fn invalid_optional_root_fields_and_bad_tile_entries_do_not_hide_good_tiles() {
    let manifest = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "python", "entry": "plugin.py", "description": {"bad": true},
            "command": 7,
            "tiles": [
                {"id": "good", "name": "Good", "hasFlyout": "yes"},
                {"id": "missing-name"},
                {"id": "good", "name": "Duplicate"}
            ]}"#,
    )
    .expect("invalid optional parts and bad tile entries are ignored");
    assert_eq!(manifest.tiles.len(), 1);
    assert_eq!(manifest.tiles[0].id, "good");
    assert!(!manifest.tiles[0].has_flyout);
    assert!(manifest.description.is_none());
    assert!(manifest.command.is_empty());
    let diagnostic = manifest.diagnostic().expect("ignored parts are reported");
    for fragment in [
        "description",
        "command",
        "hasFlyout",
        "missing field",
        "duplicate",
    ] {
        assert!(
            diagnostic.contains(fragment),
            "missing {fragment:?}: {diagnostic}"
        );
    }
}

#[test]
fn generated_manifest_schema_remains_forward_compatible() {
    let schema = serde_json::to_value(schemars::schema_for!(PluginManifest)).expect("schema");
    assert_ne!(schema["additionalProperties"], false);
    assert_ne!(
        schema["$defs"]["PluginTileDef"]["additionalProperties"],
        false
    );
}

#[test]
fn python_without_entry_and_exec_without_command_are_rejected() {
    let err = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "python", "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect_err("python without entry");
    assert!(err.to_string().contains("entry"));

    let err = load_json(
        r#"{"id": "x", "name": "X", "version": "1", "protocolVersion": 1,
            "runtime": "exec", "command": [], "tiles": [{"id": "w", "name": "W"}]}"#,
    )
    .expect_err("exec without command");
    assert!(err.to_string().contains("command"));
}
