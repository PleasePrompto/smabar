//! MCP tools for the token-based theme system. All file I/O and document
//! validation live in [`themes::io`], shared with the Tauri theme commands.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};
use serde_json::{Value, json};

use crate::themes::{self, ThemeDocument};

use super::SmabarMcp;
use super::types::{
    AckResult, ThemeGetParams, ThemeGetResult, ThemeListResult, ThemeRemoveParams, ThemeWriteParams,
};

#[tool_router(router = theme_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "List all themes: the four compiled-in full themes (default, paper, \
                       terminal, topbar) plus every drop-in theme file, each with \
                       preview colors and the active one marked. Activate a theme via \
                       settings_set(\"theme\", name)."
    )]
    pub(super) async fn theme_list(&self) -> Result<Json<ThemeListResult>, McpError> {
        let active = self.config.current().theme;
        Ok(Json(ThemeListResult {
            themes: themes::summaries(&self.paths, &active),
        }))
    }

    #[tool(
        description = "CALL THIS FIRST before creating or editing a theme. Returns the \
                       resolved `tokens`, merged behavior `settings`, `baseTheme`, and the \
                       compact authoring `contract`: format, settings allowlist and font rules. \
                       Query details with contractPath (JSON Pointer), e.g. /baseTokens/--sb-accent, \
                       /fonts, /themeSchema or /referenceThemes/paper. Large nodes return a \
                       child-path index; follow its paths or nextOffset (offset parameter). \
                       contractPath=\"\" lists the contract root. Without `name`: the active \
                       theme. A same-named drop-in patches its bundled theme; a drop-in-only \
                       theme inherits bundled default. Plugin HTML consumes tokens via \
                       var(--sb-*). For a polished look start from the closest \
                       referenceTheme (default: near-black floating dock, hairline rim, quiet \
                       tiles, the smabar yellow reserved for state; topbar: flat \
                       full-width panel at the top edge, grey mono accent; paper: light \
                       warm off-white with an ink-blue accent; terminal: monospace, \
                       square, phosphor-green strip) and keep its recipe: the tile tokens \
                       style tile cards AND shortcut dock buttons together, and \
                       tile-hover-lift/-scale set the hover pop (headroom follows \
                       automatically). For public sharing of a finished new theme, \
                       read plugin_guide(section=\"publishing\")."
    )]
    pub(super) async fn theme_get(
        &self,
        Parameters(ThemeGetParams {
            name,
            contract_path,
            offset,
        }): Parameters<ThemeGetParams>,
    ) -> Result<Json<ThemeGetResult>, McpError> {
        let name = match name {
            Some(name) => {
                validate_theme_name(&name)?;
                self.require_known_theme(&name)?;
                name
            }
            None => self.config.current().theme,
        };
        Ok(Json(ThemeGetResult {
            tokens: themes::resolve(&self.paths, &name),
            settings: themes::settings_block(&self.paths, &name),
            base_theme: if themes::is_bundled(&name) {
                name.clone()
            } else {
                "default".to_string()
            },
            meta: themes::dropin_meta(&self.paths, &name),
            contract: read_contract(contract_path.as_deref(), offset.unwrap_or(0))?,
            name,
        }))
    }

    #[tool(
        description = "Create or replace one drop-in theme file. EXACT WORKFLOW: (1) call \
                       theme_get FIRST and read its contract; query /referenceThemes/<name> \
                       with contractPath for a bundled recipe; (2) when changing \
                       fonts, call font_list and set both the family/source token pair returned \
                       by contract.fonts (`system` or `google:<catalog-id>`); (3) call \
                       theme_write with a NEW lowercase name, flat token object, optional \
                       settings, and optional meta (display name/author/version/description — \
                       fill it for shareable themes; users can export any theme as a \
                       self-contained file and import such files); (4) activate via \
                       settings_set(\"theme\", name); (5) call \
                       theme_get(name) to verify the resolved result. Omitted tokens inherit \
                       bundled default. To start from another bundled look, copy the queried \
                       reference theme's tokens and settings. Settings are \
                       dotted paths from contract.settings.allowedPaths, applied one-shot on \
                       every activation; tokens never go under appearance.tokens. Bundled \
                       names (default, paper, terminal, topbar) are read-only. Rewriting the \
                       active theme requires switching away and back to re-apply it. \
                       After creating and verifying a new theme, follow \
                       plugin_guide(section=\"publishing\").offer."
    )]
    pub(super) async fn theme_write(
        &self,
        Parameters(ThemeWriteParams {
            name,
            tokens,
            settings,
            meta,
        }): Parameters<ThemeWriteParams>,
    ) -> Result<Json<AckResult>, McpError> {
        validate_theme_name(&name)?;
        if themes::is_bundled(&name) {
            return Err(McpError::invalid_params(
                format!(
                    "\"{name}\" is a compiled-in theme and read-only; to vary it, write your \
                     tokens under a NEW name (it inherits bundled default) or patch this \
                     bundled base with a same-named drop-in file edited by hand"
                ),
                None,
            ));
        }
        let map = themes::io::parse_tokens_strict(tokens)
            .map_err(|message| McpError::invalid_params(message, None))?;
        let block = settings
            .map(|value| {
                let block = themes::settings::parse_settings_value(value)
                    .map_err(|message| McpError::invalid_params(message, None))?;
                themes::settings::validate_settings(&block).map_err(|errors| {
                    McpError::invalid_params(
                        format!("invalid theme settings: {}", errors.join("; ")),
                        None,
                    )
                })?;
                Ok::<_, McpError>(block)
            })
            .transpose()?
            .unwrap_or_default();
        let meta = meta
            .map(|value| {
                themes::io::parse_meta_value(value).map_err(|errors| {
                    McpError::invalid_params(
                        format!("invalid theme meta: {}", errors.join("; ")),
                        None,
                    )
                })
            })
            .transpose()?
            .unwrap_or_default();
        let document = ThemeDocument {
            tokens: map,
            settings: block.clone(),
            meta,
        };
        // MCP semantics stay create-or-replace; the collision confirm lives
        // in the settings UI, not here.
        let new_theme = !self
            .paths
            .themes_dir()
            .join(format!("{name}.json"))
            .exists();
        let target = themes::io::write_theme(&self.paths, &name, &document, true)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let hint = if self.config.current().theme == name {
            "this is the ACTIVE theme — explicitly switch to another theme and back to apply \
             both its tokens and activation settings (e.g. settings_set(\"theme\", \"default\") \
             and back)"
                .to_string()
        } else {
            format!("activate it via settings_set(\"theme\", \"{name}\")")
        };
        let settings_note = if block.is_empty() {
            String::new()
        } else {
            format!(
                " and {} activation setting(s) (applied one-shot on every activation)",
                block.len()
            )
        };
        let mut message = format!(
            "wrote {} with {} token(s){settings_note}; {hint}",
            target.display(),
            document.tokens.len()
        );
        if new_theme {
            message.push(' ');
            message.push_str(super::guide_tools::PUBLISHING_HINT);
        }
        Ok(Json(AckResult { message }))
    }

    #[tool(
        description = "Permanently remove one drop-in theme and its Community Store receipt. \
                       Bundled themes are read-only. If active, first activate another theme \
                       with settings_set(\"theme\", name). Missing themes are errors; theme_list \
                       shows the available names."
    )]
    pub(super) async fn theme_remove(
        &self,
        Parameters(ThemeRemoveParams { name }): Parameters<ThemeRemoveParams>,
    ) -> Result<Json<AckResult>, McpError> {
        validate_theme_name(&name)?;
        self.config.with_current(|current| {
            if !themes::is_bundled(&name) && current.theme == name {
                return Err(McpError::invalid_params(
                    format!("theme \"{name}\" is active; use settings_set(\"theme\", \"default\") or another theme before removing it"),
                    None,
                ));
            }
            themes::io::delete_theme(&self.paths, &name).map_err(|error| match error {
                themes::io::ThemeIoError::Io { .. } => McpError::internal_error(error.to_string(), None),
                error => McpError::invalid_params(error.to_string(), None),
            })
        })?;
        if let Some(store) = &self.store {
            store.note_removed();
        }
        Ok(Json(AckResult {
            message: format!("removed theme \"{name}\" and its store receipt"),
        }))
    }
}

/// Keep authoring replies readable while retaining access to every contract fact.
fn read_contract(path: Option<&str>, offset: usize) -> Result<Value, McpError> {
    const MAX_FRAGMENT_BYTES: usize = 16 * 1024;
    const INDEX_PAGE_SIZE: usize = 100;
    let mut contract = themes::contract::export();
    let Some(path) = path else {
        if offset != 0 {
            return Err(McpError::invalid_params("offset needs contractPath", None));
        }
        let paths: Vec<String> = contract
            .as_object()
            .into_iter()
            .flat_map(|object| object.keys())
            .map(|key| format!("/{key}"))
            .collect();
        if let Some(object) = contract.as_object_mut() {
            object.retain(|key, _| {
                matches!(
                    key.as_str(),
                    "version"
                        | "baseThemes"
                        | "themeFormat"
                        | "hierarchy"
                        | "settings"
                        | "fonts"
                        | "transparency"
                        | "responsiveBreakpoints"
                )
            });
            object.insert("paths".to_string(), json!(paths));
        }
        return Ok(contract);
    };
    let value = contract.pointer(path).ok_or_else(|| {
        McpError::invalid_params(
            format!("no contract value at {path:?}; use contractPath=\"\" to list available paths"),
            None,
        )
    })?;
    let size = serde_json::to_vec(value)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?
        .len();
    if size <= MAX_FRAGMENT_BYTES {
        if offset != 0 {
            return Err(McpError::invalid_params(
                "offset is only valid for a child-path index",
                None,
            ));
        }
        return Ok(value.clone());
    }
    let children: Vec<String> = match value {
        Value::Object(object) => object
            .keys()
            .map(|key| key.replace('~', "~0").replace('/', "~1"))
            .collect(),
        Value::Array(array) => (0..array.len()).map(|index| index.to_string()).collect(),
        _ => {
            return Err(McpError::internal_error(
                "a scalar contract value exceeds the 16 KiB response budget",
                None,
            ));
        }
    };
    if offset >= children.len() {
        return Err(McpError::invalid_params(
            "offset is past the end of the child-path index",
            None,
        ));
    }
    let entries: Vec<Value> = children
        .iter()
        .skip(offset)
        .take(INDEX_PAGE_SIZE)
        .map(|key| json!({"path": format!("{path}/{key}")}))
        .collect();
    let next = offset + entries.len();
    Ok(
        json!({"kind": "index", "path": path, "entries": entries, "total": children.len(),
        "nextOffset": (next < children.len()).then_some(next)}),
    )
}

impl SmabarMcp {
    /// Errors when `name` is neither the bundled default nor a drop-in file.
    fn require_known_theme(&self, name: &str) -> Result<(), McpError> {
        let known = themes::available_themes(&self.paths);
        if known.iter().any(|theme| theme == name) {
            Ok(())
        } else {
            Err(McpError::invalid_params(
                format!("no theme \"{name}\"; known themes: {}", known.join(", ")),
                None,
            ))
        }
    }
}

fn validate_theme_name(name: &str) -> Result<(), McpError> {
    if themes::is_valid_theme_name(name) {
        Ok(())
    } else {
        Err(McpError::invalid_params(
            format!("theme name \"{name}\" must be non-empty and contain only [a-z0-9-]"),
            None,
        ))
    }
}
