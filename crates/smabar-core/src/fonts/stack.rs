use super::FontCategory;

pub(super) fn family_stack(family: &str, category: FontCategory, monospaced: bool) -> String {
    if let Some(stack) = generic_stack(family) {
        return stack.to_string();
    }
    let fallback = if monospaced || category == FontCategory::Monospace {
        "ui-monospace, monospace"
    } else {
        match category {
            FontCategory::Serif => "serif",
            FontCategory::Handwriting => "cursive",
            FontCategory::SansSerif | FontCategory::Display | FontCategory::Monospace => {
                "system-ui, sans-serif"
            }
        }
    };
    format!("'{}', {fallback}", escape_family(family))
}

fn generic_stack(family: &str) -> Option<&'static str> {
    match family.to_ascii_lowercase().as_str() {
        "system-ui" | "ui-sans-serif" | "ui-rounded" => Some("system-ui, sans-serif"),
        "ui-serif" => Some("ui-serif, serif"),
        "ui-monospace" => Some("ui-monospace, monospace"),
        "serif" => Some("serif"),
        "sans-serif" => Some("sans-serif"),
        "cursive" => Some("cursive"),
        "fantasy" => Some("fantasy"),
        "monospace" => Some("monospace"),
        "math" => Some("math"),
        "fangsong" => Some("fangsong"),
        _ => None,
    }
}

fn escape_family(family: &str) -> String {
    let mut escaped = String::with_capacity(family.len());
    for character in family.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            character if character.is_control() => escaped.push('\u{fffd}'),
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_quote_names_and_keep_cross_platform_generics() {
        assert_eq!(
            family_stack("Mono ' Lab\n", FontCategory::Monospace, true),
            "'Mono \\' Lab�', ui-monospace, monospace"
        );
        assert_eq!(
            family_stack("ui-monospace", FontCategory::Monospace, true),
            "ui-monospace, monospace"
        );
    }
}
