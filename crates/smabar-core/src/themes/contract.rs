//! Machine-readable theme/token contract and canonical token validation.

#[cfg(test)]
mod census_tests;
mod schema;

use std::sync::LazyLock;

use serde_json::Value;

use super::BUNDLED;

const CONTRACT_JSON: &str = include_str!("../../../../ui-kit/theme-contract.json");
const COMPONENT_TOKENS_JSON: &str = include_str!("../../../../ui-kit/component-tokens.json");

static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(CONTRACT_JSON).unwrap_or_else(|error| {
        tracing::error!(%error, "bundled theme contract is invalid");
        Value::Null
    })
});

static COMPONENT_TOKENS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(COMPONENT_TOKENS_JSON).unwrap_or_else(|error| {
        tracing::error!(%error, "generated component token census is invalid");
        Value::Null
    })
});

/// Complete contract served over MCP. Config JSON Schema and resolved theme
/// examples are generated from the Rust types and compiled theme documents,
/// so those two sections cannot drift into prose.
pub fn export() -> Value {
    let mut contract = CONTRACT.clone();
    contract["publicComponentTokens"] = COMPONENT_TOKENS["publicComponentTokens"].clone();
    contract["internalTokens"] = COMPONENT_TOKENS["internalTokens"].clone();
    contract["themeSchema"] = schema::build(&contract, &COMPONENT_TOKENS);
    contract["configSchema"] =
        serde_json::to_value(schemars::schema_for!(crate::config::SmabarConfig))
            .unwrap_or(Value::Null);
    contract["referenceThemes"] = Value::Object(
        BUNDLED
            .iter()
            .map(|(name, theme)| {
                (
                    (*name).to_string(),
                    serde_json::json!({
                        "tokens": theme.tokens,
                        "settings": theme.settings,
                    }),
                )
            })
            .collect(),
    );
    contract["canonicalTokenNames"] = Value::Array(
        token_group("baseTokens")
            .into_iter()
            .chain(token_group("publicComponentTokens"))
            .map(Value::String)
            .collect(),
    );
    contract
}

/// Validates one theme token at the drop-in/theme-write boundary. Extension
/// namespaces stay open; smabar's own namespace is a closed public contract.
pub fn validate_token(key: &str, value: &str) -> Result<(), String> {
    if !super::is_valid_token_key(key) {
        return Err(
            "not a CSS custom-property name (expected `--` followed by [A-Za-z0-9_-])".to_string(),
        );
    }
    if !super::is_valid_token_value(value) {
        return Err("not a simple CSS value (no control characters, `;`, `{`, `}`, or url() other than data:/https:)".to_string());
    }
    if !key.starts_with("--sb-") {
        return Ok(());
    }
    if definition("internalTokens", key).is_some() {
        return Err("is internal runtime state/geometry and cannot be themed".to_string());
    }
    let base_definition = definition("baseTokens", key);
    let component_definition = definition("publicComponentTokens", key);
    let Some(definition) = base_definition.or(component_definition) else {
        return Err("is not a known public --sb-* token; use a non-sb extension namespace for plugin tokens".to_string());
    };
    if base_definition.is_none() && definition["themeable"] != true {
        return Err(
            "is component-local and cannot be set by a theme; set it on the component's style"
                .to_string(),
        );
    }
    if matches!(key, "--sb-font-sans-source" | "--sb-font-mono-source") {
        return crate::fonts::validate_source_token(value);
    }
    let allowed = &definition["allowed"];
    if let Some(values) = allowed.get("values").and_then(Value::as_array)
        && !values
            .iter()
            .any(|candidate| candidate.as_str() == Some(value))
    {
        let expected = values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!("must be one of: {expected}"));
    }
    if allowed.get("alpha").and_then(Value::as_str) == Some("opaque") && has_non_opaque_alpha(value)
    {
        return Err(
            "must be opaque, including all var() dependencies; use --sb-bar-opacity for bar, \
             tile, and inner alpha or --sb-flyout-opacity for flyout alpha"
                .to_string(),
        );
    }
    if definition["type"] == "percentage" || definition["type"] == "number" {
        let raw = if definition["type"] == "percentage" {
            value.strip_suffix('%').map(str::trim)
        } else {
            Some(value.trim())
        };
        let Some(number) = raw.and_then(|raw| raw.parse::<f64>().ok()) else {
            return Err(format!("must be a {}", definition["type"]));
        };
        let minimum = allowed["minimum"].as_f64().unwrap_or(f64::NEG_INFINITY);
        let maximum = allowed["maximum"].as_f64().unwrap_or(f64::INFINITY);
        if !(minimum..=maximum).contains(&number) {
            let unit = if definition["type"] == "percentage" {
                "%"
            } else {
                ""
            };
            return Err(format!(
                "must be between {minimum}{unit} and {maximum}{unit}"
            ));
        }
    }
    Ok(())
}

fn definition(group: &str, key: &str) -> Option<&'static Value> {
    if group == "baseTokens" {
        CONTRACT.get(group)?.get(key)
    } else {
        COMPONENT_TOKENS.get(group)?.get(key)
    }
}

fn token_group(group: &str) -> Vec<String> {
    let source = if group == "baseTokens" {
        &*CONTRACT
    } else {
        &*COMPONENT_TOKENS
    };
    source
        .get(group)
        .and_then(Value::as_object)
        .map(|tokens| tokens.keys().cloned().collect())
        .unwrap_or_default()
}

fn has_non_opaque_alpha(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    let lower = compact.to_ascii_lowercase();
    if lower.contains("transparent") || has_non_opaque_hex_alpha(&lower) {
        return true;
    }
    ["rgba(", "hsla("]
        .iter()
        .any(|function| function_has_non_opaque_alpha(&lower, function, true))
        || [
            "rgb(",
            "hsl(",
            "hwb(",
            "lab(",
            "lch(",
            "oklab(",
            "oklch(",
            "color(",
            "device-cmyk(",
            "gray(",
        ]
        .iter()
        .any(|function| function_has_non_opaque_alpha(&lower, function, false))
        || has_unsafe_var_reference(&compact, &lower)
}

fn has_non_opaque_hex_alpha(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'#' {
            cursor += 1;
            continue;
        }
        let start = cursor + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        let hex = &value[start..end];
        if (hex.len() == 4 && !hex.ends_with('f')) || (hex.len() == 8 && !hex.ends_with("ff")) {
            return true;
        }
        cursor = end.max(cursor + 1);
    }
    false
}

fn function_has_non_opaque_alpha(value: &str, function: &str, comma_alpha: bool) -> bool {
    let mut cursor = 0;
    while let Some(offset) = value[cursor..].find(function) {
        let open = cursor + offset + function.len() - 1;
        let Some(arguments) = function_arguments(value, open) else {
            return true;
        };
        let alpha = last_top_level_separator(arguments, '/')
            .map(|separator| &arguments[separator + 1..])
            .or_else(|| {
                (comma_alpha && top_level_separator_count(arguments, ',') >= 3)
                    .then(|| last_top_level_separator(arguments, ','))
                    .flatten()
                    .map(|separator| &arguments[separator + 1..])
            });
        if alpha.is_some_and(|alpha| !alpha_is_opaque(alpha)) {
            return true;
        }
        cursor = open + arguments.len() + 2;
    }
    false
}

fn function_arguments(value: &str, open: usize) -> Option<&str> {
    let mut depth = 0_usize;
    for (offset, byte) in value.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return value.get(open + 1..open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn last_top_level_separator(value: &str, separator: char) -> Option<usize> {
    let mut depth = 0_usize;
    let mut found = None;
    for (index, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            character if character == separator && depth == 0 => found = Some(index),
            _ => {}
        }
    }
    found
}

fn top_level_separator_count(value: &str, separator: char) -> usize {
    let mut depth = 0_usize;
    value
        .chars()
        .filter(|character| match character {
            '(' => {
                depth += 1;
                false
            }
            ')' => {
                depth = depth.saturating_sub(1);
                false
            }
            character => *character == separator && depth == 0,
        })
        .count()
}

fn has_unsafe_var_reference(value: &str, lower: &str) -> bool {
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find("var(") {
        let open = cursor + offset + 3;
        let Some(arguments) = function_arguments(value, open) else {
            return true;
        };
        let name = last_top_level_separator(arguments, ',')
            .map_or(arguments, |separator| &arguments[..separator]);
        let opaque_public_token = definition("baseTokens", name)
            .or_else(|| definition("publicComponentTokens", name))
            .is_some_and(|definition| definition["allowed"]["alpha"] == "opaque");
        if !opaque_public_token {
            return true;
        }
        cursor = open + arguments.len() + 2;
    }
    false
}

fn alpha_is_opaque(alpha: &str) -> bool {
    alpha
        .strip_suffix('%')
        .and_then(|value| value.parse::<f64>().ok())
        .is_some_and(|value| (value - 100.0).abs() < f64::EPSILON)
        || alpha
            .parse::<f64>()
            .is_ok_and(|value| (value - 1.0).abs() < f64::EPSILON)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn opaque_surface_validation_rejects_alpha_but_accepts_opaque_css_colors() {
        for value in [
            "rgba(38, 22, 12, .6)",
            "#26160c99",
            "transparent",
            "color-mix(in srgb, rgba(38, 22, 12, .6), #fff)",
            "linear-gradient(#26160c99, #fff)",
            "rgb(38 22 12 / calc(.5))",
            "var(--my-plugin-surface)",
            "var(--sb-flyout-shadow)",
            "var(--sb-chart-track)",
        ] {
            assert!(validate_token("--sb-bar-bg", value).is_err(), "{value}");
        }
        assert!(validate_token("--sb-text", "rgba(255, 255, 255, .4)").is_err());
        for value in [
            "#26160c",
            "rgb(38 22 12)",
            "rgba(38,22,12)",
            "rgba(38,22,12,1.0)",
            "color-mix(in srgb, var(--sb-bar-bg) 96%, var(--sb-text))",
            "var(--sb-flyout-bg)",
            "red",
        ] {
            assert!(validate_token("--sb-bar-bg", value).is_ok(), "{value}");
        }
    }

    #[test]
    fn every_canonical_opaque_default_passes_dependency_validation() {
        let base = CONTRACT["baseTokens"].as_object().expect("base tokens");
        for (name, definition) in base {
            if definition["allowed"]["alpha"] == "opaque" {
                let value = definition["default"].as_str().expect("string default");
                assert!(validate_token(name, value).is_ok(), "{name}: {value}");
            }
        }
    }

    #[test]
    fn exported_theme_schema_is_draft_2020_12_and_tracks_metadata() {
        let contract = export();
        let schema = &contract["themeSchema"];
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["$ref"], "#/$defs/dropIn");
        assert!(contract["themeFormat"].is_object());

        let properties = schema["$defs"]["dropIn"]["properties"]
            .as_object()
            .expect("drop-in properties");
        assert!(properties.contains_key("--sb-color-scheme"));
        assert!(properties.contains_key("--sb-chart-1"));
        assert!(properties.contains_key("--sb-gap"));
        assert!(properties.contains_key("--sb-select-list-max"));
        assert!(properties.contains_key("settings"));
        assert!(properties.contains_key("meta"));
        assert_eq!(properties["meta"]["additionalProperties"], true);
        assert!(!properties.contains_key("--sb-autohide-edge-gap"));
        assert!(!properties.contains_key("--sb-chart-max"));
        assert!(properties["--sb-gap"].get("default").is_none());
        assert!(properties["--sb-gap"]["x-contextualDefaults"].is_array());
        assert!(properties["--sb-gap"]["x-cssConsumers"].is_array());
        assert_eq!(
            properties["--sb-color-scheme"]["enum"],
            serde_json::json!(["dark", "light"])
        );
        assert_eq!(
            properties["settings"]["properties"]
                .as_object()
                .map(serde_json::Map::len),
            Some(crate::themes::settings::ALLOWED_PATHS.len())
        );
        assert_eq!(schema["$defs"]["dropIn"]["additionalProperties"], false);

        let required = schema["$defs"]["bundled"]["allOf"][1]["required"]
            .as_array()
            .expect("bundled required")
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(required.len(), token_group("baseTokens").len() + 1);
        assert!(required.contains("settings"));
        let required_settings =
            schema["$defs"]["bundled"]["allOf"][1]["properties"]["settings"]["required"]
                .as_array()
                .expect("bundled settings required");
        assert_eq!(
            required_settings.len(),
            crate::themes::settings::ALLOWED_PATHS.len()
        );
    }

    #[test]
    fn namespace_is_closed_but_extensions_remain_open() {
        assert!(validate_token("--sb-select-list-max", "14rem").is_ok());
        assert!(validate_token("--sb-scale", "1.25").is_ok());
        assert!(validate_token("--sb-scale", "0.5").is_err());
        assert!(validate_token("--sb-chart-1", "#0af").is_ok());
        assert!(validate_token("--sb-autohide-edge-gap", "1rem").is_err());
        assert!(validate_token("--sb-chart-max", "10").is_err());
        assert!(validate_token("--sb-not-real", "1rem").is_err());
        assert!(validate_token("--my-plugin-glow", "0 0 8px red").is_ok());
    }
}
