//! MCP tool serving the plugin authoring contract (`plugin-guide/guide.json`
//! plus the template plugin, all compiled in).
//!
//! `ui_kit` answers "what may my HTML look like"; this answers "how do I write
//! a plugin at all" — the manifest schema, the SDK API, the lifecycle
//! guarantees, and a working plugin to start from.

use std::sync::LazyLock;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};
use serde_json::{Value, json};

use crate::plugins::PluginManifest;

use super::SmabarMcp;
use super::plugin_types::{GuideParams, GuideResult};

const GUIDE_JSON: &str = include_str!("../../../../plugin-guide/guide.json");
/// Pointer only: offer timing, consent and publishing steps belong to the guide.
pub(super) const PUBLISHING_HINT: &str =
    "After final verification, follow plugin_guide(section=\"publishing\").offer.";
/// The template ships as real files so it stays runnable and diffable; a test
/// pushes the manifest through the production validator.
pub(super) const TEMPLATE_MANIFEST: &str =
    include_str!("../../../../plugin-guide/template/smabar.json");
pub(super) const TEMPLATE_SCRIPT: &str =
    include_str!("../../../../plugin-guide/template/plugin.py");
pub(super) const TEMPLATE_VIEWS: &str = include_str!("../../../../plugin-guide/template/views.py");
pub(super) const TEMPLATE_LOCALE_EN: &str =
    include_str!("../../../../plugin-guide/template/locales/en.json");
pub(super) const TEMPLATE_LOCALE_DE: &str =
    include_str!("../../../../plugin-guide/template/locales/de.json");

/// The template's files in the order they are written: every sibling first,
/// the manifest last, because the manifest write is the start.
pub(super) const TEMPLATE_FILES: &[(&str, &str)] = &[
    ("views.py", TEMPLATE_VIEWS),
    ("locales/en.json", TEMPLATE_LOCALE_EN),
    ("locales/de.json", TEMPLATE_LOCALE_DE),
    ("plugin.py", TEMPLATE_SCRIPT),
    ("smabar.json", TEMPLATE_MANIFEST),
];

/// What the template reply says about copying it.
pub(super) const TEMPLATE_NOTE: &str = "Write these files in the listed order with plugin_write_file — siblings first, \
     smabar.json LAST (its write starts the plugin). Rename: the folder = manifest id, \
     TILE = the tile id, the \"folder.\" locale prefix. Keep the shape: first render in \
     @app.on_ready from the cache, slow work in measure() on a thread, os.replace for the \
     cache, set_settings spreads app.settings, action() for every icon-only button. \
     plugin.py explains the optional app icon and the separate cover choice; the full \
     contract is plugin_guide(section=\"manifest\").folderIcon.";

pub(super) static GUIDE: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(GUIDE_JSON).unwrap_or_else(|error| {
        // Unreachable in a healthy build: a test asserts the bundle parses.
        tracing::error!(%error, "bundled plugin-guide/guide.json is invalid");
        Value::Null
    })
});

/// `start` is the default: the steps with their completion criteria plus
/// the capabilities index. Every other name discloses one reference block.
const SECTIONS: &[&str] = &[
    "start",
    "capabilities",
    "manifest",
    "sdk",
    "storage",
    "lifecycle",
    "debugging",
    "publishing",
    "template",
    "all",
];

#[tool_router(router = guide_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "The START document for building a plugin: numbered golden-path steps, \
                       each with a checkable completion criterion and the reference it needs, \
                       plus the terms the other replies use, the markup rules in brief and the \
                       capabilities index (topics with read calls, live host services and \
                       providers). Call it without `section` first. `section` discloses one \
                       reference block instead: manifest (fields, optional app/cover icons, \
                       validation, the settings panel's renderable subset, multi-file plugins, \
                       the generated JSON Schema), sdk (every public smabar_sdk.Plugin member), storage \
                       (app.data_dir, SQLite, atomic writes, stale-while-refresh, network in a \
                       thread), lifecycle (startup order, the ONE handler lock, hot reload, no \
                       shutdown hook), debugging (plugin_list, plugin_logs, plugin_data, common \
                       failures), template (a complete multi-file plugin in write order), \
                       publishing (optional sharing of plugins AND themes), capabilities \
                       (the index alone), all (everything at once). Every reply \
                       carries the golden path. No bundled plugin is required; installed ones \
                       are worked examples readable with plugin_read."
    )]
    pub(super) async fn plugin_guide(
        &self,
        Parameters(GuideParams { section }): Parameters<GuideParams>,
    ) -> Result<Json<GuideResult>, McpError> {
        let section = section.unwrap_or_else(|| "start".to_string());
        if !SECTIONS.contains(&section.as_str()) {
            return Err(McpError::invalid_params(
                format!(
                    "unknown section \"{section}\"; expected one of {}",
                    SECTIONS.join(", ")
                ),
                None,
            ));
        }
        let start = section == "start";
        let wants = |name: &str| section == "all" || section == name;
        let field = |key: &str| GUIDE.get(key).cloned();
        let capabilities = (start || wants("capabilities")).then(|| {
            let mut capabilities = GUIDE["capabilities"].clone();
            capabilities["runtime"] = json!({
                "services": self.supervisor.capabilities(),
                "availableProviders": self.hub.available_names(),
            });
            capabilities
        });
        Ok(Json(GuideResult {
            version: GUIDE.get("version").and_then(Value::as_u64).unwrap_or(1),
            // The path leads every reply, whatever section was requested: the
            // steps are what a narrowed reply is a reference FOR.
            golden_path: field("goldenPath"),
            terms: (start || wants("all")).then(|| field("terms")).flatten(),
            // The style rules ride with the start: an agent that reads only
            // this tool would otherwise design its markup before ui_kit.
            style: (start || wants("all")).then(|| field("style")).flatten(),
            sections: start.then(|| field("sections")).flatten(),
            capabilities,
            manifest: wants("manifest").then(|| field("manifest")).flatten(),
            manifest_schema: wants("manifest").then(manifest_schema),
            sdk: wants("sdk").then(|| field("sdk")).flatten(),
            storage: wants("storage").then(|| field("storage")).flatten(),
            lifecycle: wants("lifecycle").then(|| field("lifecycle")).flatten(),
            debugging: wants("debugging").then(|| field("debugging")).flatten(),
            publishing: wants("publishing").then(|| field("publishing")).flatten(),
            template: wants("template").then(|| {
                json!({
                    "note": TEMPLATE_NOTE,
                    "files": TEMPLATE_FILES
                        .iter()
                        .map(|(path, content)| json!({"path": path, "content": content}))
                        .collect::<Vec<_>>(),
                })
            }),
        }))
    }
}

/// JSON Schema of the manifest, generated from the struct the core actually
/// parses — so it can never drift from what validation accepts.
fn manifest_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(PluginManifest)).unwrap_or(Value::Null)
}
