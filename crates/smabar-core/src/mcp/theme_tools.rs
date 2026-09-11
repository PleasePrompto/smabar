//! MCP tools for the token-based theme system. All file I/O and document
//! validation live in [`themes::io`], shared with the Tauri theme commands.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::themes::{self, ThemeDocument};

use super::SmabarMcp;
use super::types::{AckResult, ThemeGetParams, ThemeGetResult, ThemeListResult, ThemeWriteParams};

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
                       complete machine-readable `contract`: token definitions and validation, \
                       settings allowlist, JSON Schema, bundled referenceThemes, and font \
                       pair/discovery/download/fallback rules. Without `name`: the active \
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
                       automatically)."
    )]
    pub(super) async fn theme_get(
        &self,
        Parameters(ThemeGetParams { name }): Parameters<ThemeGetParams>,
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
            contract: themes::contract::export(),
            name,
        }))
    }

    #[tool(
        description = "Create or replace one drop-in theme file. EXACT WORKFLOW: (1) call \
                       theme_get FIRST and use its contract/referenceThemes; (2) when changing \
                       fonts, call font_list and set both the family/source token pair returned \
                       by contract.fonts (`system` or `google:<catalog-id>`); (3) call \
                       theme_write with a NEW lowercase name, flat token object, optional \
                       settings, and optional meta (display name/author/version/description — \
                       fill it for shareable themes; users can export any theme as a \
                       self-contained file and import such files); (4) activate via \
                       settings_set(\"theme\", name); (5) call \
                       theme_get(name) to verify the resolved result. Omitted tokens inherit \
                       bundled default. To start from another bundled look, copy that \
                       contract.referenceThemes entry's tokens and settings. Settings are \
                       dotted paths from contract.settings.allowedPaths, applied one-shot on \
                       every activation; tokens never go under appearance.tokens. Bundled \
                       names (default, paper, terminal, topbar) are read-only. Rewriting the \
                       active theme requires switching away and back to re-apply it."
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
        Ok(Json(AckResult {
            message: format!(
                "wrote {} with {} token(s){settings_note}; {hint}",
                target.display(),
                document.tokens.len()
            ),
        }))
    }
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
