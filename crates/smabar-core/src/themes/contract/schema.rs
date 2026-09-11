//! Draft 2020-12 schema generated from the canonical contract metadata.

use serde_json::{Map, Value, json};

const EXTENSION_TOKEN_PATTERN: &str = r"^--(?!sb-)[A-Za-z0-9_-]+$";
const SAFE_VALUE_PATTERN: &str = r"^[^;{}\u0000-\u001F\u007F]+$";

pub(super) fn build(contract: &Value, component_tokens: &Value) -> Value {
    let base = contract["baseTokens"].as_object();
    let public = component_tokens["publicComponentTokens"].as_object();
    let settings = contract["settings"]["allowedPaths"].as_object();

    let mut properties = Map::new();
    for (name, definition) in base.into_iter().flat_map(|values| values.iter()) {
        properties.insert(name.clone(), token_schema(definition));
    }
    for (name, definition) in public.into_iter().flat_map(|values| values.iter()) {
        if definition["themeable"] == true {
            properties.insert(name.clone(), token_schema(definition));
        }
    }
    properties.insert("settings".to_string(), settings_schema(settings, false));
    properties.insert("meta".to_string(), meta_schema());

    let drop_in = json!({
        "type": "object",
        "description": "A partial theme. Missing base tokens inherit from the selected bundled base.",
        "properties": properties,
        "patternProperties": {
            EXTENSION_TOKEN_PATTERN: extension_token_schema(),
        },
        "additionalProperties": false,
    });

    let required_tokens = base
        .into_iter()
        .flat_map(|definitions| definitions.keys())
        .cloned()
        .chain(std::iter::once("settings".to_string()))
        .map(Value::String)
        .collect::<Vec<_>>();
    let required_settings = settings
        .into_iter()
        .flat_map(|definitions| definitions.keys())
        .cloned()
        .map(Value::String)
        .collect::<Vec<_>>();
    let bundled = json!({
        "allOf": [
            { "$ref": "#/$defs/dropIn" },
            {
                "required": required_tokens,
                "properties": {
                    "settings": { "required": required_settings },
                },
            },
        ],
    });

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Smabar theme document",
        "description": "Flat CSS custom properties plus the reserved settings object. Use the bundled definition when validating a compiled base theme.",
        "$ref": "#/$defs/dropIn",
        "$defs": {
            "dropIn": drop_in,
            "bundled": bundled,
        },
        "x-entrypoints": {
            "dropIn": "#/$defs/dropIn",
            "bundled": "#/$defs/bundled",
        },
        "$comment": "CSS grammar, opaque-surface dependency checks, URL schemes and percentage numeric ranges receive additional runtime validation.",
    })
}

fn token_schema(definition: &Value) -> Value {
    let mut schema = Map::from_iter([
        ("type".to_string(), json!("string")),
        ("minLength".to_string(), json!(1)),
        ("pattern".to_string(), json!(SAFE_VALUE_PATTERN)),
    ]);
    copy_as(definition, "default", &mut schema, "default");
    copy_as(definition, "meaning", &mut schema, "description");
    copy_as(definition, "type", &mut schema, "x-tokenType");
    copy_as(definition, "group", &mut schema, "x-group");
    copy_as(definition, "scope", &mut schema, "x-scope");
    copy_as(definition, "themeable", &mut schema, "x-themeable");
    copy_as(definition, "dependencies", &mut schema, "x-dependencies");
    copy_as(definition, "consumers", &mut schema, "x-consumers");
    copy_as(definition, "cssConsumers", &mut schema, "x-cssConsumers");
    copy_as(
        definition,
        "contextualDefaults",
        &mut schema,
        "x-contextualDefaults",
    );
    if let Some(values) = definition["allowed"]["values"].as_array() {
        schema.insert("enum".to_string(), Value::Array(values.clone()));
    }
    if let Some(allowed) = definition.get("allowed") {
        schema.insert("x-cssAllowed".to_string(), allowed.clone());
    }
    Value::Object(schema)
}

fn meta_schema() -> Value {
    let field = |description: &str| {
        json!({
            "type": "string",
            "minLength": 1,
            "maxLength": 200,
            "description": description,
        })
    };
    json!({
        "type": "object",
        "description": "Optional self-describing metadata. Ignored by the resolver, preserved \
                        on export/import; fill it for themes meant to be shared.",
        "properties": {
            "name": field("Display name; the theme's identity stays the file stem."),
            "author": field("Who made this theme."),
            "version": field("Free-form version string."),
            "description": field("One-line description of the look."),
        },
        // Unknown meta fields stay open for future store additions.
        "additionalProperties": true,
    })
}

fn extension_token_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "pattern": SAFE_VALUE_PATTERN,
        "description": "Plugin-specific extension token. The --sb-* namespace remains closed.",
    })
}

fn settings_schema(definitions: Option<&Map<String, Value>>, require_all: bool) -> Value {
    let properties = definitions
        .into_iter()
        .flat_map(|definitions| definitions.iter())
        .map(|(path, definition)| (path.clone(), setting_schema(definition)))
        .collect::<Map<_, _>>();
    let mut schema = Map::from_iter([
        ("type".to_string(), json!("object")),
        ("properties".to_string(), Value::Object(properties)),
        ("additionalProperties".to_string(), json!(false)),
    ]);
    if require_all {
        schema.insert(
            "required".to_string(),
            Value::Array(
                definitions
                    .into_iter()
                    .flat_map(|definitions| definitions.keys())
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    Value::Object(schema)
}

fn setting_schema(definition: &Value) -> Value {
    let json_type = match definition["type"].as_str() {
        Some("enum") => "string",
        Some("boolean") => "boolean",
        Some("integer") => "integer",
        Some("number") => "number",
        _ => "string",
    };
    let mut schema = Map::from_iter([("type".to_string(), json!(json_type))]);
    copy_as(definition, "default", &mut schema, "default");
    copy_as(definition, "meaning", &mut schema, "description");
    copy_as(definition, "unit", &mut schema, "x-unit");
    for field in ["minimum", "maximum"] {
        copy_as(definition, field, &mut schema, field);
    }
    if let Some(values) = definition["values"].as_array() {
        if let Some(range) = definition["range"].as_object() {
            let mut ranged = Map::from_iter([("type".to_string(), json!(json_type))]);
            for field in ["minimum", "maximum"] {
                if let Some(value) = range.get(field) {
                    ranged.insert(field.to_string(), value.clone());
                }
            }
            schema.insert(
                "anyOf".to_string(),
                json!([{ "enum": values }, Value::Object(ranged)]),
            );
        } else {
            schema.insert("enum".to_string(), Value::Array(values.clone()));
        }
    }
    Value::Object(schema)
}

fn copy_as(source: &Value, source_key: &str, target: &mut Map<String, Value>, target_key: &str) {
    if let Some(value) = source.get(source_key).filter(|value| !value.is_null()) {
        target.insert(target_key.to_string(), value.clone());
    }
}
