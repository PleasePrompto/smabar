//! Font catalog and cross-platform system-font discovery.
//!
//! This module is deliberately network-free. The Tauri app edge downloads a
//! known Google family and stores it under [`SmabarPaths::google_fonts_dir`].

mod stack;

/// How a Google font is named in a theme's font-source token. The font
/// tools, the theme tools and the theme contract all spell it this way.
pub const GOOGLE_FONT_TOKEN_SYNTAX: &str = "google:<catalog-id>";

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::SmabarPaths;

const GOOGLE_CATALOG_JSON: &str = include_str!("google-fonts.json");

/// Source shown by the font picker and accepted by the Tauri list command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FontSource {
    System,
    Google,
}

/// Broad Google Fonts categories; system faces can only reliably identify
/// monospace, so other installed faces use the closest generic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FontCategory {
    SansSerif,
    Serif,
    Monospace,
    Display,
    Handwriting,
}

/// One entry returned to the settings picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontOption {
    /// Stable Google catalog id, or `system:<family>` for a system entry.
    /// System ids identify search results; their theme source token is still
    /// the literal `system`.
    pub id: String,
    /// Human-readable canonical family name; use `cssStack` for a theme token.
    pub family: String,
    /// Ready-to-write, safely quoted CSS family list with a portable generic
    /// fallback.
    pub css_stack: String,
    /// Where smabar resolves this family.
    pub source: FontSource,
    /// Broad category used to choose a portable CSS generic fallback.
    pub category: FontCategory,
    /// Whether the family is suitable for the theme's mono slot.
    pub monospaced: bool,
    /// Google cache marker exists locally; system entries are always true.
    pub cached: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoogleCatalog {
    version: u8,
    source: String,
    family_count: usize,
    families: Vec<GoogleFont>,
}

/// Checked-in Google family metadata used to construct an allowlisted CSS2
/// request. File URLs never come from themes or config.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleFont {
    id: String,
    family: String,
    category: FontCategory,
    monospaced: bool,
    variants: Vec<String>,
    popularity: usize,
}

impl GoogleFont {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn family(&self) -> &str {
        &self.family
    }

    /// CSS2 `family` query value requesting real regular/bold and italic
    /// faces where the family provides them. The HTTP layer URL-encodes it.
    pub fn css2_family_spec(&self) -> String {
        let normal = preferred_weights(&self.variants, false);
        let italic = preferred_weights(&self.variants, true);
        let tuples = normal
            .iter()
            .map(|weight| format!("0,{weight}"))
            .chain(italic.iter().map(|weight| format!("1,{weight}")))
            .collect::<Vec<_>>();
        if normal.is_empty() {
            format!("{}:ital,wght@{}", self.family, tuples.join(";"))
        } else if italic.is_empty() {
            format!(
                "{}:wght@{}",
                self.family,
                normal
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(";")
            )
        } else {
            format!("{}:ital,wght@{}", self.family, tuples.join(";"))
        }
    }

    fn css_stack(&self, mono_slot: bool) -> String {
        stack::family_stack(&self.family, self.category, mono_slot || self.monospaced)
    }
}

static GOOGLE_CATALOG: LazyLock<GoogleCatalog> = LazyLock::new(|| {
    let parsed = serde_json::from_str::<GoogleCatalog>(GOOGLE_CATALOG_JSON);
    match parsed {
        Ok(catalog)
            if catalog.version == 1
                && catalog.family_count == catalog.families.len()
                && catalog.source.starts_with("https://") =>
        {
            catalog
        }
        Ok(_) => {
            tracing::error!("bundled Google Fonts catalog metadata is inconsistent");
            empty_google_catalog()
        }
        Err(error) => {
            tracing::error!(%error, "bundled Google Fonts catalog is invalid");
            empty_google_catalog()
        }
    }
});

fn empty_google_catalog() -> GoogleCatalog {
    GoogleCatalog {
        version: 1,
        source: String::new(),
        family_count: 0,
        families: Vec::new(),
    }
}

/// Known family by stable catalog id.
pub fn google_font(id: &str) -> Option<&'static GoogleFont> {
    GOOGLE_CATALOG.families.iter().find(|font| font.id == id)
}

/// Validate the metadata token paired with a theme font-family token.
pub fn validate_source_token(value: &str) -> Result<(), String> {
    if value == "system" {
        return Ok(());
    }
    let Some(id) = value.strip_prefix("google:") else {
        return Err(format!("must be `system` or `{GOOGLE_FONT_TOKEN_SYNTAX}`"));
    };
    if google_font(id).is_none() {
        return Err(format!(
            "unknown Google Fonts catalog id {id:?}; choose an id returned by font_list"
        ));
    }
    Ok(())
}

/// Force a known Google source and its family to agree. This is applied after
/// theme overlay resolution, so a foreign theme cannot download font A while
/// asking CSS to render unrelated font B.
pub fn canonicalize_theme_fonts(tokens: &mut BTreeMap<String, String>) {
    for (family_token, source_token, mono_slot) in [
        ("--sb-font-sans", "--sb-font-sans-source", false),
        ("--sb-font-mono", "--sb-font-mono-source", true),
    ] {
        let Some(id) = tokens
            .get(source_token)
            .and_then(|source| source.strip_prefix("google:"))
        else {
            continue;
        };
        if let Some(font) = google_font(id) {
            tokens.insert(family_token.to_string(), font.css_stack(mono_slot));
        }
    }
}

/// Picker entries from portable CSS generics, installed OS fonts, and the
/// checked-in Google catalog. Filters run in core so a future MCP frontend can
/// reuse exactly the same behavior.
pub fn font_options(
    paths: &SmabarPaths,
    query: Option<&str>,
    source: Option<FontSource>,
    monospaced: Option<bool>,
    limit: Option<usize>,
) -> Vec<FontOption> {
    let query = query.unwrap_or_default().trim().to_lowercase();
    let mut options = Vec::new();
    if source != Some(FontSource::Google) {
        options.extend(system_options());
    }
    if source != Some(FontSource::System) {
        let mut google_fonts = GOOGLE_CATALOG.families.iter().collect::<Vec<_>>();
        if query.is_empty() {
            google_fonts.sort_by_key(|font| font.popularity);
        } else {
            google_fonts.sort_by_key(|font| font.family.to_lowercase());
        }
        options.extend(google_fonts.into_iter().map(|font| FontOption {
            id: font.id.clone(),
            family: font.family.clone(),
            css_stack: font.css_stack(false),
            source: FontSource::Google,
            category: font.category,
            monospaced: font.monospaced,
            cached: false,
        }));
    }
    let mut options: Vec<FontOption> = options
        .into_iter()
        .filter(|font| {
            (query.is_empty()
                || font.family.to_lowercase().contains(&query)
                || font.id.contains(&query))
                && monospaced.is_none_or(|wanted| font.monospaced == wanted)
        })
        .take(limit.unwrap_or(50).min(100))
        .collect();
    let google_fonts_dir = paths.google_fonts_dir();
    for font in &mut options {
        if font.source == FontSource::Google {
            font.cached = google_fonts_dir
                .join(&font.id)
                .join("manifest.json")
                .is_file();
        }
    }
    options
}

#[derive(Debug)]
struct SystemFamily {
    family: String,
    monospaced: bool,
}

static SYSTEM_FAMILIES: LazyLock<Vec<SystemFamily>> = LazyLock::new(|| {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    aggregate_system_families(database.faces().flat_map(|face| {
        face.families
            .iter()
            .map(move |(family, _language)| (family.as_str(), face.monospaced))
    }))
});

fn aggregate_system_families<'a>(
    faces: impl Iterator<Item = (&'a str, bool)>,
) -> Vec<SystemFamily> {
    let mut families = BTreeMap::<String, SystemFamily>::new();
    for (family, monospaced) in faces {
        let family = family.trim();
        if family.is_empty() {
            continue;
        }
        let key = family.to_lowercase();
        families
            .entry(key)
            .and_modify(|known| known.monospaced |= monospaced)
            .or_insert_with(|| SystemFamily {
                family: family.to_string(),
                monospaced,
            });
    }
    families.into_values().collect()
}

fn system_options() -> Vec<FontOption> {
    let generics = [
        ("system-ui", FontCategory::SansSerif, false),
        ("sans-serif", FontCategory::SansSerif, false),
        ("serif", FontCategory::Serif, false),
        ("ui-monospace", FontCategory::Monospace, true),
        ("monospace", FontCategory::Monospace, true),
    ];
    let mut seen = BTreeSet::new();
    let mut options = Vec::new();
    for (family, category, monospaced) in generics {
        seen.insert(family.to_string());
        options.push(system_option(family, category, monospaced));
    }
    for font in SYSTEM_FAMILIES.iter() {
        if !seen.insert(font.family.to_lowercase()) {
            continue;
        }
        let category = if font.monospaced {
            FontCategory::Monospace
        } else if font.family.to_lowercase().contains("serif")
            && !font.family.to_lowercase().contains("sans")
        {
            FontCategory::Serif
        } else {
            FontCategory::SansSerif
        };
        options.push(system_option(&font.family, category, font.monospaced));
    }
    options
}

fn system_option(family: &str, category: FontCategory, monospaced: bool) -> FontOption {
    FontOption {
        id: format!("system:{family}"),
        family: family.to_string(),
        css_stack: stack::family_stack(family, category, monospaced),
        source: FontSource::System,
        category,
        monospaced,
        cached: true,
    }
}

fn preferred_weights(variants: &[String], italic: bool) -> Vec<u16> {
    let available = variants
        .iter()
        .filter_map(|variant| {
            let is_italic = variant.ends_with('i');
            (is_italic == italic)
                .then(|| variant.trim_end_matches('i').parse::<u16>().ok())
                .flatten()
        })
        .collect::<Vec<_>>();
    let targets: &[u16] = if italic {
        &[400]
    } else {
        &[400, 500, 600, 700]
    };
    let mut selected = targets
        .iter()
        .copied()
        .filter_map(|target| {
            available
                .iter()
                .min_by_key(|weight| weight.abs_diff(target))
                .copied()
        })
        .collect::<Vec<_>>();
    selected.sort_unstable();
    selected.dedup();
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_catalog_is_full_and_ids_are_unique() {
        assert!(GOOGLE_CATALOG.family_count >= 1_900);
        let ids = GOOGLE_CATALOG
            .families
            .iter()
            .map(|font| font.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), GOOGLE_CATALOG.families.len());
        assert_eq!(
            google_font("roboto").map(GoogleFont::family),
            Some("Roboto")
        );
    }

    #[test]
    fn removed_catalog_fields_are_rejected() {
        let root = serde_json::json!({
            "version": 1, "source": "https://example.com", "familyCount": 0,
            "activeFamilyCount": 0, "families": []
        });
        assert!(serde_json::from_value::<GoogleCatalog>(root).is_err());

        let family = serde_json::json!({
            "id": "demo", "family": "Demo", "category": "sans-serif",
            "monospaced": false, "variants": ["regular"], "popularity": 0,
            "deprecated": true
        });
        assert!(serde_json::from_value::<GoogleFont>(family).is_err());
    }

    #[test]
    fn css2_spec_uses_real_regular_bold_and_italic_faces() {
        assert_eq!(
            google_font("roboto").map(GoogleFont::css2_family_spec),
            Some("Roboto:ital,wght@0,400;0,500;0,600;0,700;1,400".to_string())
        );
        assert_eq!(
            google_font("abeezee").map(GoogleFont::css2_family_spec),
            Some("ABeeZee:ital,wght@0,400;1,400".to_string())
        );
    }

    #[test]
    fn source_tokens_accept_only_system_or_a_known_catalog_id() {
        assert!(validate_source_token("system").is_ok());
        assert!(validate_source_token("google:roboto").is_ok());
        assert!(validate_source_token("google:not-a-real-font").is_err());
        assert!(validate_source_token("https://example.com/font.woff2").is_err());
    }

    #[test]
    fn google_source_canonicalizes_a_mismatched_foreign_theme() {
        let mut tokens = BTreeMap::from([
            (
                "--sb-font-sans".to_string(),
                "Totally Different".to_string(),
            ),
            (
                "--sb-font-sans-source".to_string(),
                "google:roboto".to_string(),
            ),
        ]);
        canonicalize_theme_fonts(&mut tokens);
        assert_eq!(
            tokens.get("--sb-font-sans").map(String::as_str),
            Some("'Roboto', system-ui, sans-serif")
        );
    }

    #[test]
    fn system_face_names_are_deduplicated_and_monospace_wins() {
        let families = aggregate_system_families(
            [("Example", false), ("example", true), ("", false)].into_iter(),
        );
        assert_eq!(families.len(), 1);
        assert_eq!(families[0].family, "Example");
        assert!(families[0].monospaced);
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let options = font_options(
            &paths,
            Some("ui-monospace"),
            Some(FontSource::System),
            None,
            Some(1),
        );
        assert!(options[0].monospaced);
        assert!(options[0].cached);
    }

    #[test]
    fn google_only_search_marks_cache_without_a_system_font_scan() {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let cache_dir = paths.google_fonts_dir().join("roboto");
        std::fs::create_dir_all(&cache_dir).expect("create cache directory");
        std::fs::write(cache_dir.join("manifest.json"), b"{}").expect("write cache marker");
        let options = font_options(
            &paths,
            Some("roboto"),
            Some(FontSource::Google),
            None,
            Some(4),
        );
        assert!(!options.is_empty());
        assert!(options.iter().all(|font| font.source == FontSource::Google));
        assert!(options.iter().all(|font| font.family.contains("Roboto")));
        assert!(
            options
                .iter()
                .any(|font| font.id == "roboto" && font.cached)
        );
    }

    #[test]
    fn list_limits_are_bounded_and_empty_google_search_uses_popularity() {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        let defaults = font_options(&paths, None, Some(FontSource::Google), None, None);
        assert_eq!(defaults.len(), 50);
        assert_eq!(defaults[0].id, "roboto");
        let capped = font_options(
            &paths,
            None,
            Some(FontSource::Google),
            None,
            Some(usize::MAX),
        );
        assert_eq!(capped.len(), 100);
    }
}
