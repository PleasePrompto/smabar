use std::collections::BTreeSet;
use std::path::Path;

use super::*;

#[test]
fn generated_component_census_covers_every_production_token() {
    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/src");
    let used = css_token_names(&production_sources(&shell));
    let base: BTreeSet<_> = token_group("baseTokens").into_iter().collect();
    let public: BTreeSet<_> = token_group("publicComponentTokens").into_iter().collect();
    let internal: BTreeSet<_> = token_group("internalTokens").into_iter().collect();
    let documented: BTreeSet<_> = base
        .iter()
        .chain(&public)
        .chain(&internal)
        .cloned()
        .collect();
    let missing: Vec<_> = used.difference(&documented).collect();
    assert!(
        missing.is_empty(),
        "undocumented production tokens: {missing:?}"
    );
    for (scope, tokens) in [("public", public), ("internal", internal)] {
        let stale: Vec<_> = tokens.difference(&used).collect();
        assert!(stale.is_empty(), "stale {scope} token metadata: {stale:?}");
        for name in tokens {
            let definition = definition(
                if scope == "public" {
                    "publicComponentTokens"
                } else {
                    "internalTokens"
                },
                &name,
            )
            .expect("definition");
            for field in [
                "scope",
                "themeable",
                "type",
                "allowed",
                "meaning",
                "group",
                "consumers",
                "cssConsumers",
                "dependencies",
            ] {
                assert!(!definition[field].is_null(), "{name} misses {field}");
            }
            if scope == "public" {
                assert!(
                    !definition["default"].is_null()
                        || definition["contextualDefaults"]
                            .as_array()
                            .is_some_and(|values| !values.is_empty()),
                    "{name} needs a default or contextualDefaults"
                );
            }
        }
    }
}

fn production_sources(directory: &Path) -> String {
    let mut source = String::new();
    for entry in std::fs::read_dir(directory).expect("read shell source directory") {
        let entry = entry.expect("read shell source entry");
        let path = entry.path();
        if path.is_dir() {
            source.push_str(&production_sources(&path));
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let extension = path.extension().and_then(|value| value.to_str());
        if name.contains(".test.") || !matches!(extension, Some("css" | "ts" | "tsx")) {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read shell production source");
        let content = strip_css_comments(&content);
        for line in content.lines() {
            if !line.trim_start().starts_with("//") {
                source.push_str(line);
                source.push('\n');
            }
        }
    }
    source
}

fn css_token_names(source: &str) -> BTreeSet<String> {
    let bytes = source.as_bytes();
    let mut tokens = BTreeSet::new();
    let mut start = 0;
    while let Some(offset) = source[start..].find("--sb-") {
        let token_start = start + offset;
        let mut end = token_start + 5;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'-' | b'_'))
        {
            end += 1;
        }
        let token = &source[token_start..end];
        if !token.ends_with('-') {
            tokens.insert(token.to_string());
        }
        start = end;
    }
    tokens
}

fn strip_css_comments(source: &str) -> String {
    let mut clean = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("/*") {
        clean.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find("*/") else {
            return clean;
        };
        rest = &rest[start + end + 4..];
    }
    clean.push_str(rest);
    clean
}
