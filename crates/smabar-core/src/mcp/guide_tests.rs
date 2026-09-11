//! Anti-drift tests for the plugin authoring guide.
//!
//! The guide is hand-written prose about code that changes. These tests fail
//! when it stops describing reality: the template must survive the production
//! manifest validator, and the documented SDK surface must match the SDK.

use std::collections::BTreeSet;
use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use crate::plugins::PluginManifest;
use crate::providers::{BatteryData, BatteryInfo, BatteryState};

use super::guide_tools::{GUIDE, TEMPLATE_MANIFEST, TEMPLATE_SCRIPT};
use super::plugin_types::GuideParams;
use super::tests::{test_handler, unwrap_json};

/// The SDK source, read at compile time so a rename cannot slip through.
const SDK_SOURCE: &str = include_str!("../../../../sdk/python/smabar_sdk/plugin.py");

#[test]
fn the_bundled_guide_parses_and_covers_every_section() {
    assert_eq!(GUIDE["version"], 1);
    for key in [
        "terms",
        "goldenPath",
        "style",
        "sections",
        "capabilities",
        "manifest",
        "sdk",
        "storage",
        "lifecycle",
        "debugging",
    ] {
        assert!(GUIDE.get(key).is_some(), "guide.json is missing {key}");
    }
    let path = GUIDE["goldenPath"]
        .as_array()
        .expect("goldenPath is a list");
    assert_eq!(path.len(), 6, "the golden path is six ordered steps");
    for (index, step) in path.iter().enumerate() {
        assert_eq!(step["step"], index as u64 + 1);
        for key in ["do", "done", "read"] {
            assert!(step.get(key).is_some(), "step {} misses {key}", index + 1);
        }
        assert!(
            step["done"].as_str().unwrap_or_default().len() > 60,
            "a completion criterion names what to check, step {}",
            index + 1
        );
    }
}

#[test]
fn the_template_passes_the_production_manifest_validator() {
    let manifest = PluginManifest::parse(TEMPLATE_MANIFEST, Path::new("smabar.json"))
        .expect("the shipped template manifest must be valid");
    assert_eq!(manifest.entry.as_deref(), Some("plugin.py"));
    let tile = manifest.tiles.first().expect("template declares a tile");
    assert!(!tile.use_plugin_icon && tile.icon_svg.is_none());
    // A render into an undeclared tile is dropped, so the template's script
    // and its manifest have to agree on the id.
    assert!(
        TEMPLATE_SCRIPT.contains(&format!("TILE = \"{}\"", tile.id)),
        "the template script must render into its declared tile"
    );
    assert!(
        TEMPLATE_SCRIPT.starts_with("# /// script"),
        "a python plugin needs the PEP 723 header uv reads"
    );
    assert!(
        TEMPLATE_SCRIPT.contains("app.run()"),
        "the template must actually serve the RPC loop"
    );
}

#[tokio::test]
async fn manifest_guide_and_schema_expose_the_cover_icon_choices() {
    let (_dir, mcp) = test_handler().await;
    let guide = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("manifest".to_string()),
        }))
        .await,
    )
    .expect("manifest guide");
    let schema = guide.manifest_schema.expect("generated manifest schema");
    assert!(
        schema
            .pointer("/$defs/PluginTileDef/properties/iconSvg")
            .is_some(),
        "generated schema must expose iconSvg: {schema}"
    );
    let flag = schema
        .pointer("/$defs/PluginTileDef/properties/usePluginIcon")
        .expect("generated schema exposes the cover opt-in");
    assert_eq!(flag["type"], "boolean");
    assert_eq!(flag["default"], false);
    let manifest = guide.manifest.expect("manifest prose");
    let prose = manifest.to_string();
    for detail in [
        "iconSvg",
        "usePluginIcon",
        "false by default",
        "8192",
        "one <svg> root",
        "external URLs",
    ] {
        assert!(prose.contains(detail), "manifest guide misses {detail}");
    }
    let examples = manifest["folderIcon"]["coverExamples"]
        .as_object()
        .expect("cover examples");
    assert_eq!(examples.len(), 3);
    for (choice, tile) in examples {
        let input = json!({
            "id": "example", "name": "Example", "version": "1",
            "protocolVersion": 1, "runtime": "exec", "command": ["example"],
            "tiles": [tile],
        });
        let parsed = PluginManifest::parse(&input.to_string(), Path::new("smabar.json"))
            .expect("documented tile example is valid");
        assert!(
            parsed.diagnostic().is_none(),
            "{choice}: {:?}",
            parsed.diagnostic()
        );
        assert_eq!(parsed.tiles[0].use_plugin_icon, choice == "pluginImage");
        assert_eq!(parsed.tiles[0].icon_svg.is_some(), choice == "customSvg");
    }
}

#[test]
fn every_documented_sdk_member_exists_in_the_sdk() {
    let documented = GUIDE["sdk"].as_object().expect("sdk section is an object");
    for name in documented.keys() {
        if name == "import" {
            continue; // prose, not a member
        }
        let defined = SDK_SOURCE.contains(&format!("    def {name}("))
            || SDK_SOURCE.contains(&format!("    def {name}(self"))
            || SDK_SOURCE.contains(&format!("self.{name} ="));
        assert!(
            defined,
            "plugin_guide documents `{name}`, but smabar_sdk.Plugin has no such member"
        );
    }
}

#[test]
fn the_guide_leads_with_the_layout_rules() {
    // An agent that reads only plugin_guide still has to learn that spacing
    // and type come from the kit — otherwise it writes inline styles and the
    // user is back to correcting it by hand.
    let style = GUIDE["style"]
        .as_object()
        .expect("style section is an object");
    let all = style
        .values()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    for needed in [
        "sb-field",
        "sb-inline",
        "sb-push",
        "sb-text-",
        "sb-reveal",
        "sb-tile-stack",
        "ui_kit",
        // The tile cover recipes: undocumented here, an agent designs tiles
        // by hand and never finds them.
        "coverLayouts",
    ] {
        assert!(
            all.contains(needed),
            "the style block never mentions {needed}"
        );
    }
    // The user-facing layout switch: a coverLayout enum in settingsSchema.
    let manifest_prose = serde_json::to_string(&GUIDE["manifest"]).expect("manifest section");
    assert!(
        manifest_prose.contains("coverLayout"),
        "the manifest guide never names the coverLayout settings convention"
    );
    // The rule is stated as what style= IS for, not as a ban to work around.
    assert!(
        all.contains("style= carries only"),
        "the style block never says what inline style is for"
    );
}

/// A plugin can only emit HTML strings, so the interactive patterns it CAN
/// build are pure-HTML ones — and a fresh agent has no way to guess them.
/// The guide is where it finds out, or it never does.
#[test]
fn the_guide_teaches_the_script_free_interaction_patterns() {
    let style = GUIDE["style"]
        .as_object()
        .expect("style section is an object");
    let all = style
        .values()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        all.contains("HTML only"),
        "the guide must say a plugin is HTML only"
    );
    for needed in [
        "<details",
        "<summary>",
        "<dialog",
        "commandfor",
        "popover",
        "popovertarget",
        "data-action",
    ] {
        assert!(
            all.contains(needed),
            "the style block never mentions {needed}"
        );
    }
    // The command values live in the kit contract; the guide points there.
    assert!(all.contains("ui_kit(sections=[\"sanitizer\"])"));
    // 200+ classes are only usable through the category sections.
    assert!(
        all.contains("ui_kit(sections=["),
        "the guide must show how to fetch ONE class category"
    );
}

#[test]
fn every_public_sdk_member_is_documented() {
    let documented = GUIDE["sdk"].as_object().expect("sdk section is an object");
    for line in SDK_SOURCE.lines() {
        let Some(rest) = line.strip_prefix("    def ") else {
            continue;
        };
        let Some(name) = rest.split('(').next() else {
            continue;
        };
        if name.starts_with('_') {
            continue; // internals
        }
        assert!(
            documented.contains_key(name),
            "the SDK exposes `{name}`, but plugin_guide does not document it — an agent \
             reading only the guide would never find it"
        );
    }
}

#[test]
fn the_guide_documents_worker_safe_output_methods() {
    for method in ["render", "log"] {
        let documented = GUIDE["sdk"][method]
            .as_str()
            .unwrap_or_else(|| panic!("sdk.{method} is not documented"));
        assert!(
            documented.contains("Thread-safe") && documented.contains("worker thread"),
            "sdk.{method} must explicitly permit calls from worker threads"
        );
    }
}

#[test]
fn the_guide_teaches_a_stable_latest_media_poll() {
    let recipe = GUIDE["storage"]["latestMedia"]
        .as_str()
        .expect("storage.latestMedia is documented");
    for needed in [
        "@app.every",
        "ONE worker",
        "timeout",
        "bounded, validated response",
        "app.data_dir",
        "os.replace",
        "stale cache",
        "stable media id",
        "unchanged id",
        "separate hover",
        "iframe",
        "external link",
        "official documentation",
        "authentication",
        "quotas",
        "best-effort",
    ] {
        assert!(
            recipe.contains(needed),
            "latest-media recipe misses {needed}"
        );
    }
}

#[test]
fn the_guide_documents_the_exact_provider_action_contracts() {
    let documented = GUIDE["sdk"]["provider_action"]
        .as_str()
        .expect("sdk.provider_action is documented");
    for needed in [
        "play",
        "pause",
        "playPause",
        "next",
        "previous",
        "sessionId",
        "currentSessionId",
        "setVolume",
        "volumePercent",
        "setMuted",
        "muted",
        "RpcError",
    ] {
        assert!(
            documented.contains(needed),
            "provider_action documentation never mentions {needed}"
        );
    }
}

#[test]
fn the_guide_documents_the_exact_battery_provider_contract() {
    let documented = GUIDE["sdk"]["on_provider"]
        .as_str()
        .expect("sdk.on_provider is documented");
    let payload = serde_json::to_value(BatteryData {
        batteries: vec![BatteryInfo {
            charge_percent: 73.5,
            health_percent: 91.0,
            cycle_count: Some(120),
            state: BatteryState::Discharging,
            is_charging: false,
            time_till_empty: Some(7_200_000.0),
            time_till_full: None,
            power_consumption: 8.5,
            voltage: 12.25,
        }],
    })
    .expect("battery payload serializes");
    let fields = payload["batteries"][0]
        .as_object()
        .expect("battery entry is an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected_fields = [
        "chargePercent",
        "cycleCount",
        "healthPercent",
        "isCharging",
        "powerConsumption",
        "state",
        "timeTillEmpty",
        "timeTillFull",
        "voltage",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(fields, expected_fields, "battery JSON fields changed");

    assert!(
        documented.contains(r#"{"batteries": [...]}"#),
        "battery payload shape is missing: {documented}"
    );
    for field in expected_fields {
        assert!(
            documented.contains(field),
            "battery field {field} is undocumented: {documented}"
        );
    }

    let states = [
        BatteryState::Charging,
        BatteryState::Discharging,
        BatteryState::Full,
        BatteryState::Empty,
        BatteryState::Unknown,
    ]
    .map(|state| serde_json::to_value(state).expect("battery state serializes"));
    assert_eq!(
        states,
        [
            serde_json::json!("charging"),
            serde_json::json!("discharging"),
            serde_json::json!("full"),
            serde_json::json!("empty"),
            serde_json::json!("unknown"),
        ],
        "battery state JSON values changed"
    );
    for state in states {
        let state = state.as_str().expect("battery state is a string");
        assert!(
            documented.contains(&format!("\"{state}\"")),
            "battery state {state} is undocumented: {documented}"
        );
    }

    for statement in [
        "may contain multiple entries",
        "empty batteries list means no battery is currently present",
        "normal data, not an error",
        "chargePercent and healthPercent are percentages",
        "cycleCount is an integer count or null",
        "isCharging is a boolean",
        "timeTillEmpty and timeTillFull are milliseconds (ms) or null",
        "powerConsumption is watts",
        "voltage is volts",
    ] {
        assert!(
            documented.contains(statement),
            "battery contract misses {statement:?}: {documented}"
        );
    }
}

#[tokio::test]
async fn sections_narrow_the_reply_but_keep_the_golden_path() {
    let (_dir, mcp) = test_handler().await;
    let template = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("template".to_string()),
        }))
        .await,
    )
    .expect("template section");
    assert!(template.template.is_some());
    assert!(template.sdk.is_none(), "a narrowed section stays narrow");
    assert!(
        template.golden_path.is_some(),
        "the golden path leads every reply"
    );

    let start = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams { section: None }))
            .await,
    )
    .expect("default section");
    assert!(start.terms.is_some() && start.style.is_some() && start.sections.is_some());
    assert!(start.capabilities.is_some());
    assert!(
        start.manifest_schema.is_none() && start.template.is_none() && start.sdk.is_none(),
        "the start document discloses the reference behind section pointers"
    );

    let all = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("all".to_string()),
        }))
        .await,
    )
    .expect("all");
    assert!(
        all.manifest_schema.is_some(),
        "manifest schema is generated"
    );
    assert!(all.template.is_some() && all.sdk.is_some() && all.style.is_some());
}

#[tokio::test]
async fn an_unknown_section_names_the_valid_ones() {
    let (_dir, mcp) = test_handler().await;
    let error = unwrap_json(
        mcp.plugin_guide(Parameters(GuideParams {
            section: Some("nonsense".to_string()),
        }))
        .await,
    )
    .expect_err("an unknown section must be refused");
    assert!(error.message.contains("template"), "{}", error.message);
}

/// Missing or unstyled markup is otherwise silent, so the guide has to say
/// where the explanation is.
#[test]
fn the_guide_points_at_the_report_for_removed_markup() {
    let markup = GUIDE["debugging"]["markup"]
        .as_str()
        .expect("debugging.markup is documented");
    assert!(markup.contains("sanitizer"), "{markup}");
    assert!(markup.contains("Unknown sb-*"), "{markup}");
    assert!(markup.contains("shell"), "{markup}");
    assert!(
        markup.contains("once"),
        "the once-per-problem guarantee is what makes it readable: {markup}"
    );
}
