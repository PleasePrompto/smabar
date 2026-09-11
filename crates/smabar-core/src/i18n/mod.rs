//! Flat key-value i18n: bundled locales plus optional drop-in overrides.
//!
//! English is the source of truth and fallback. German is bundled as a second
//! language; additional languages are drop-in JSON files in
//! [`SmabarPaths::locales_dir`]. Per-key drop-in overrides win. A broken
//! drop-in never breaks the bar — it logs a warning and keeps the best bundled
//! fallback.

use std::collections::BTreeMap;
use std::fs;
use std::sync::LazyLock;

use crate::config::SmabarPaths;

/// Flat mapping of i18n keys to translated strings.
pub type LocaleMap = BTreeMap<String, String>;

const BUNDLED_EN_JSON: &str = include_str!("../../../../locales/en.json");
const BUNDLED_DE_JSON: &str = include_str!("../../../../locales/de.json");
const MISSING_LOCALE_WARNING: &str = "configured locale file is missing; add a flat JSON locale file at this path or set \"language\" to \"en\" or \"de\"; using English fallback";

static BUNDLED_EN: LazyLock<LocaleMap> = LazyLock::new(|| {
    serde_json::from_str(BUNDLED_EN_JSON).unwrap_or_else(|error| {
        // Unreachable in a healthy build: a test asserts the bundle parses.
        tracing::error!(%error, "bundled locales/en.json is invalid; UI strings will be missing");
        LocaleMap::new()
    })
});

static BUNDLED_DE: LazyLock<LocaleMap> = LazyLock::new(|| {
    serde_json::from_str(BUNDLED_DE_JSON).unwrap_or_else(|error| {
        // Unreachable in a healthy build: a test asserts the bundle parses.
        tracing::error!(%error, "bundled locales/de.json is invalid; German UI strings will fall back to English");
        LocaleMap::new()
    })
});

/// The bundled English locale — the source of truth for all UI strings.
pub fn bundled_english() -> &'static LocaleMap {
    &BUNDLED_EN
}

/// Resolve `language`: English fallback, bundled translation, then drop-in.
pub fn resolve(paths: &SmabarPaths, language: &str) -> LocaleMap {
    let mut map = BUNDLED_EN.clone();
    if language == "en" {
        return map;
    }
    if !is_valid_language_code(language) {
        tracing::warn!(language, "invalid language code; falling back to English");
        return map;
    }
    let has_bundled_translation = language == "de";
    if has_bundled_translation {
        map.extend(BUNDLED_DE.clone());
    }
    let file = paths.locales_dir().join(format!("{language}.json"));
    let raw = match fs::read_to_string(&file) {
        Ok(raw) => raw,
        // Bundled locales need no drop-in override. A configured language
        // without either source is different: silence would hide the typo or
        // missing installation that caused the English fallback.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !has_bundled_translation {
                tracing::warn!(language, path = %file.display(), "{}", MISSING_LOCALE_WARNING);
            }
            return map;
        }
        Err(error) => {
            tracing::warn!(
                %error,
                path = %file.display(),
                "locale drop-in not readable; using bundled locale fallback"
            );
            return map;
        }
    };
    match serde_json::from_str::<LocaleMap>(&raw) {
        Ok(dropin) => map.extend(dropin),
        Err(error) => tracing::warn!(
            %error,
            path = %file.display(),
            "locale drop-in is not a flat JSON string map; using bundled locale fallback"
        ),
    }
    map
}

/// Bundled languages plus every `*.json` drop-in, sorted and deduplicated.
pub fn available_languages(paths: &SmabarPaths) -> Vec<String> {
    let mut languages = vec!["en".to_string(), "de".to_string()];
    if let Ok(entries) = fs::read_dir(paths.locales_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
            {
                languages.push(stem.to_string());
            }
        }
    }
    languages.sort();
    languages.dedup();
    languages
}

/// Language codes are plain file stems (`de`, `pt-BR`, …) — never paths.
pub(crate) fn is_valid_language_code(language: &str) -> bool {
    !language.is_empty()
        && language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let paths = SmabarPaths::new(dir.path().join("smabar"));
        (dir, paths)
    }

    fn write_locale(paths: &SmabarPaths, language: &str, content: &str) {
        let dir = paths.locales_dir();
        fs::create_dir_all(&dir).expect("create locales dir");
        fs::write(dir.join(format!("{language}.json")), content).expect("write locale");
    }

    fn bundled_german_over_english() -> LocaleMap {
        let mut map = BUNDLED_EN.clone();
        map.extend(BUNDLED_DE.clone());
        map
    }

    #[test]
    fn bundled_english_parses_and_is_non_empty() {
        let bundle = bundled_english();
        assert!(!bundle.is_empty());
        assert_eq!(
            bundle.get("app.hello").map(String::as_str),
            Some("Hello smabar")
        );
    }

    #[test]
    fn resolve_en_returns_the_bundle() {
        let (_dir, paths) = temp_paths();
        assert_eq!(&resolve(&paths, "en"), bundled_english());
    }

    #[test]
    fn bundled_german_has_the_exact_english_keyset_and_resolves_without_dropin() {
        assert!(!BUNDLED_DE.is_empty());
        assert!(BUNDLED_DE.values().all(|value| !value.is_empty()));
        assert!(BUNDLED_EN.keys().eq(BUNDLED_DE.keys()));

        let (_dir, paths) = temp_paths();
        let map = resolve(&paths, "de");
        assert_eq!(map, bundled_german_over_english());
        assert_eq!(
            map.get("settings.close").map(String::as_str),
            Some("Einstellungen schließen")
        );
    }

    #[test]
    fn missing_locale_warning_names_both_recovery_paths() {
        let (_dir, paths) = temp_paths();
        assert_eq!(resolve(&paths, "de"), bundled_german_over_english());
        assert_eq!(&resolve(&paths, "fr"), bundled_english());
        assert!(
            MISSING_LOCALE_WARNING.contains("add a flat JSON locale file")
                && MISSING_LOCALE_WARNING.contains("set \"language\" to \"en\" or \"de\""),
            "the warning must name both supported recovery paths"
        );
    }

    #[test]
    fn german_dropin_overrides_the_bundled_translation() {
        let (_dir, paths) = temp_paths();
        write_locale(&paths, "de", r#"{"settings.close":"Zumachen"}"#);

        let map = resolve(&paths, "de");
        assert_eq!(
            map.get("settings.close").map(String::as_str),
            Some("Zumachen")
        );
        assert_eq!(
            map.get("settings.title").map(String::as_str),
            Some("Einstellungen")
        );
        assert_eq!(map.len(), bundled_english().len());
    }

    #[test]
    fn resolve_merges_other_dropin_over_english_fallback() {
        let (_dir, paths) = temp_paths();
        write_locale(&paths, "nl", r#"{"app.hello":"Hallo smabar"}"#);

        let map = resolve(&paths, "nl");
        assert_eq!(
            map.get("app.hello").map(String::as_str),
            Some("Hallo smabar")
        );
        assert_eq!(
            map.get("settings.close").map(String::as_str),
            Some("Close settings")
        );
        assert_eq!(map.len(), bundled_english().len());
    }

    #[test]
    fn broken_dropins_keep_the_best_bundled_fallback() {
        let (_dir, paths) = temp_paths();
        write_locale(&paths, "de", "{ not json");
        assert_eq!(resolve(&paths, "de"), bundled_german_over_english());

        // Structurally valid JSON but not a flat string map is also "broken".
        write_locale(&paths, "nl", r#"{"app.hello":{"nested":true}}"#);
        assert_eq!(&resolve(&paths, "nl"), bundled_english());
    }

    #[test]
    fn resolve_falls_back_to_english_for_missing_or_invalid_language() {
        let (_dir, paths) = temp_paths();
        assert_eq!(&resolve(&paths, "fr"), bundled_english());
        assert_eq!(&resolve(&paths, "../evil"), bundled_english());
        assert_eq!(&resolve(&paths, ""), bundled_english());
    }

    #[test]
    fn available_languages_lists_bundles_plus_dropins_without_duplicates() {
        let (_dir, paths) = temp_paths();
        assert_eq!(available_languages(&paths), vec!["de", "en"]);

        write_locale(&paths, "de", "{}");
        write_locale(&paths, "nl", "{}");
        write_locale(&paths, "en", "{}");
        fs::write(paths.locales_dir().join("notes.txt"), "ignore me").expect("write txt");

        assert_eq!(available_languages(&paths), vec!["de", "en", "nl"]);
    }
}
