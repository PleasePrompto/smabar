//! Behavior tests for the MCP tool logic, calling the handler methods
//! directly (the HTTP transport is rmcp's tested code).

use std::sync::Arc;

use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::{Json, Parameters};
use serde_json::json;

use crate::capture::{BarPort, BarRequest, BarResponse, Screenshot};
use crate::config::{BarPosition, ConfigWatcher, SmabarPaths};
use crate::platform::Rect;
use crate::plugins::{PluginSupervisor, SupervisorOptions};
use crate::providers::ProviderHub;
use crate::shortcuts::{IconDirs, ShortcutsService};

use super::SmabarMcp;
use super::types::{
    BarSetPositionParams, PluginOrderSetParams, SettingsGetParams, SettingsSetParams,
};

pub(super) async fn test_handler() -> (tempfile::TempDir, SmabarMcp) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("spawn watcher"));
    let hub = ProviderHub::new();
    let supervisor = PluginSupervisor::start(
        paths.clone(),
        hub.clone(),
        Arc::clone(&config),
        SupervisorOptions::default(),
    )
    .await;
    // Shortcut fixtures live in <base>/applications and <base>/pixmaps;
    // tests write .desktop files there and call `shortcuts.refresh()`.
    let platform = crate::platform::shortcuts::xdg(
        vec![paths.base_dir().join("applications")],
        IconDirs {
            theme_roots: Vec::new(),
            flat_dirs: vec![paths.base_dir().join("pixmaps")],
            ..IconDirs::default()
        },
        None,
        paths.base_dir().to_path_buf(),
    );
    let shortcuts = ShortcutsService::new(platform, paths.icons_dir());
    (
        dir,
        SmabarMcp::new(paths, config, hub, supervisor, shortcuts, fake_bar()),
    )
}

/// The smallest possible PNG: enough to prove the image reaches the caller
/// without a window in the test process.
pub(super) const ONE_PIXEL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n-not-a-real-png";

/// Stands in for the running app: answers the bar and one plugin tile,
/// rejecting every other target the way the shell would.
pub(super) fn fake_bar() -> BarPort {
    let (port, mut rx) = BarPort::channel();
    tokio::spawn(async move {
        while let Some((request, reply)) = rx.recv().await {
            let targets = vec!["bar".to_string(), "plugin:clock:clock".to_string()];
            let answer = match request {
                BarRequest::Screenshot { target, scale } if targets.contains(&target) => {
                    Ok(BarResponse::Screenshot(Box::new(Screenshot {
                        png: ONE_PIXEL_PNG.to_vec(),
                        rect: Rect {
                            x: 0,
                            y: 0,
                            w: 120,
                            h: 40,
                        },
                        pixels: (120 * u32::from(scale), 40 * u32::from(scale)),
                        expanded: false,
                        clipped: false,
                        targets,
                    })))
                }
                BarRequest::Screenshot { target, .. } => {
                    Err(crate::capture::BarError::UnknownTarget {
                        target,
                        available: targets,
                    })
                }
                BarRequest::UiState { .. } => Ok(BarResponse::UiState { targets }),
            };
            let _ = reply.send(answer);
        }
    });
    port
}

/// `rmcp::Json` has no `Debug`; unwrap it so `expect`/`expect_err` work.
pub(super) fn unwrap_json<T>(result: Result<Json<T>, McpError>) -> Result<T, McpError> {
    result.map(|json| json.0)
}

#[tokio::test]
async fn settings_get_returns_whole_config_and_dotted_paths() {
    let (_dir, mcp) = test_handler().await;

    let whole = unwrap_json(
        mcp.settings_get(Parameters(SettingsGetParams { path: None }))
            .await,
    )
    .expect("whole config");
    assert_eq!(whole.value["layout"]["position"], json!("bottom"));
    assert_eq!(whole.value["mcp"]["port"], json!(7627));
    assert_eq!(whole.value["settingsWindow"]["width"], json!(960));
    assert_eq!(whole.value["settingsWindow"]["height"], json!(680));

    let port = unwrap_json(
        mcp.settings_get(Parameters(SettingsGetParams {
            path: Some("mcp.port".to_string()),
        }))
        .await,
    )
    .expect("dotted path");
    assert_eq!(port.value, json!(7627));

    let err = unwrap_json(
        mcp.settings_get(Parameters(SettingsGetParams {
            path: Some("plugins.nope.city".to_string()),
        }))
        .await,
    )
    .expect_err("missing path");
    assert!(err.message.contains("plugins.nope.city"));
}

#[tokio::test]
async fn settings_set_rejects_typo_roots_and_invalid_enum_values() {
    let (_dir, mcp) = test_handler().await;

    let err = unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "moode".to_string(),
            value: json!("top"),
        }))
        .await,
    )
    .expect_err("typo root");
    assert!(err.message.contains(
        "zOrder, language, theme, themeExportDir, pluginOrder, plugins, mcp, rendering, \
         layout, shortcuts, pluginsHidden, pluginsDeactivated, effects"
    ));

    let err = unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "layout.position".to_string(),
            value: json!("sideways"),
        }))
        .await,
    )
    .expect_err("invalid enum value");
    assert!(err.message.contains("rejected"));
    // The rejected value must not have been persisted.
    assert_eq!(mcp.config.current().layout.position, BarPosition::Bottom);

    // Intermediates are only created under plugins.
    let err = unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "mcp.nested.thing".to_string(),
            value: json!(1),
        }))
        .await,
    )
    .expect_err("missing intermediate outside plugins");
    assert!(err.message.contains("nested"));
}

#[tokio::test]
async fn settings_set_rejects_json_encoded_string_values() {
    let (_dir, mcp) = test_handler().await;

    // Regression: a client serialized the `value` parameter as a JSON STRING
    // ("{\"sendungen\": ...}") — the string landed in the config unnoticed
    // and the plugin never saw its object/array.
    for encoded in [r#"{"sendungen": ["156510313639"]}"#, r#"  ["a", "b"]"#] {
        let err = unwrap_json(
            mcp.settings_set(Parameters(SettingsSetParams {
                path: "plugins.hello.data".to_string(),
                value: json!(encoded),
            }))
            .await,
        )
        .expect_err(&format!("encoded value {encoded:?} must be rejected"));
        assert!(
            err.message.contains("JSON-encoded string"),
            "message: {}",
            err.message
        );
    }
    assert!(!mcp.config.current().plugins.contains_key("hello"));

    // A plain string value still passes.
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "language".to_string(),
            value: json!("de"),
        }))
        .await,
    )
    .expect("plain string value");
    assert_eq!(mcp.config.current().language, "de");

    // A real object passes and lands as an object.
    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "plugins.hello.data".to_string(),
            value: json!({"sendungen": ["156510313639"]}),
        }))
        .await,
    )
    .expect("real object value");
    assert_eq!(
        mcp.config.current().plugins.get("hello"),
        Some(&json!({"data": {"sendungen": ["156510313639"]}}))
    );
}

#[test]
fn settings_set_value_schema_names_all_json_types() {
    // A bare `serde_json::Value` field derives the empty schema `{}`, which
    // makes some MCP clients serialize objects as JSON-encoded strings.
    let schema = schemars::schema_for!(SettingsSetParams);
    let types = schema
        .as_value()
        .pointer("/properties/value/type")
        .expect("value property carries a type list");
    assert_eq!(
        types,
        &json!(["object", "array", "string", "number", "boolean", "null"])
    );
}

#[tokio::test]
async fn settings_set_applies_values_and_creates_plugin_objects() {
    let (_dir, mcp) = test_handler().await;

    unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "plugins.hello.city".to_string(),
            value: json!("Berlin"),
        }))
        .await,
    )
    .expect("set plugin setting");
    assert_eq!(
        mcp.config.current().plugins.get("hello"),
        Some(&json!({ "city": "Berlin" }))
    );

    let ack = unwrap_json(
        mcp.settings_set(Parameters(SettingsSetParams {
            path: "mcp.port".to_string(),
            value: json!(9000),
        }))
        .await,
    )
    .expect("set mcp port");
    assert_eq!(mcp.config.current().mcp.port, 9000);
    assert!(ack.message.contains("restart"));
}

#[tokio::test]
async fn bar_set_position_sets_and_broadcasts_the_position() {
    let (_dir, mcp) = test_handler().await;
    let mut changes = mcp.config.subscribe();

    for (position, name) in [(BarPosition::Top, "top"), (BarPosition::Bottom, "bottom")] {
        let ack = unwrap_json(
            mcp.bar_set_position(Parameters(BarSetPositionParams { position }))
                .await,
        )
        .expect("set position");
        assert_eq!(mcp.config.current().layout.position, position);
        assert!(ack.message.contains(&format!("layout.position={name}")));
        // The mutation is broadcast so the app's config-event forwarders fire.
        let change = changes.try_recv().expect("layout change must broadcast");
        assert!(change.layout_changed());
    }

    assert!(
        serde_json::from_value::<BarSetPositionParams>(json!({"position": "sideways"})).is_err()
    );
}

#[tokio::test]
async fn plugin_order_set_persists_the_order() {
    let (_dir, mcp) = test_handler().await;
    let order = vec![
        "plugin:clock:clock".to_string(),
        "plugin:hello:main".to_string(),
    ];
    unwrap_json(
        mcp.plugin_order_set(Parameters(PluginOrderSetParams {
            order: order.clone(),
        }))
        .await,
    )
    .expect("set tile order");
    assert_eq!(mcp.config.current().plugin_order, order);
}
