//! The server instructions handed to every connected MCP client, and the
//! design prompt derived from the guide.
//!
//! Clients TRUNCATE the instructions, so the opening sentences must carry
//! the whole golden path on their own. A test pins the first 512 characters
//! and another the word budget.

/// What smabar tells a connecting agent about itself.
///
/// Pointer tier only: the golden path, where each reference lives, and the
/// two SDK traps no tool reply can raise. Every other fact has exactly one
/// owner — the tool it belongs to, plugin_guide or ui_kit — and a word
/// budget test keeps this text from growing back.
pub(super) const INSTRUCTIONS: &str = "\
smabar is a desktop bar whose tiles are plugins; the user supplies the idea. \
Golden path, in order: (1) plugin_guide — the START document: steps with completion \
criteria plus the capabilities index; no bundled plugin is required. \
(2) ui_kit(sections=[\"coverLayouts\",\"bestPractices\",\"<class category>\",\"snippets\"]) \
before any HTML — one call; a bare ui_kit() answers with the index. \
(3) plugin_write_file: siblings first (views.py, locales/en.json, plugin.py), smabar.json \
LAST — that write starts the plugin and its reply is your test result. (4) Clean start: \
reload.warnings.count is 0; otherwise fix the first warning and write again. \
(5) Photograph every surface after the last write: reload.designReview lists the exact \
bar_screenshot and bar_ui_state calls; inspect each PNG.\n\
Pointers: plugin_guide(section=manifest|sdk|storage|lifecycle|debugging|template) for \
building; ui_kit(sections=[…]) for markup, behaviour hooks and conventions; theme_get FIRST \
for themes and fonts; plugin_guide(section=\"publishing\") for public sharing of either. \
External services: current official documentation. Installed \
plugins: plugin_list, then plugin_commands(id) and plugin_call for their data operations.\n\
Two traps no tool reply can raise: app.set_settings replaces the whole settings object — \
write {**app.settings, key: value}; the first render and every app.t() belong in \
@app.on_ready, because settings and locales exist only after initialize.";

/// The `design_plugin` prompt, generated from the guide's golden path so
/// the two can never drift: same steps, same completion criteria.
pub(super) fn design_plugin_prompt(golden_path: &serde_json::Value) -> String {
    let steps = golden_path
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .map(|step| {
                    format!(
                        "{}. {} Done when: {}",
                        step["step"],
                        step["do"].as_str().unwrap_or_default(),
                        step["done"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    format!(
        "Design or revise the current smabar plugin. Follow the golden path completely — \
         it is plugin_guide's goldenPath, so the two cannot drift:\n{steps}"
    )
}

#[cfg(test)]
mod tests {
    use super::INSTRUCTIONS;

    #[test]
    fn the_opening_survives_truncation_with_the_golden_path_intact() {
        // Chars, not bytes: the text contains multi-byte punctuation.
        let opening: String = INSTRUCTIONS.chars().take(512).collect();
        for needle in [
            "plugin_guide",
            "ui_kit",
            "coverLayouts",
            "bestPractices",
            "plugin_write_file",
            "smabar.json LAST",
        ] {
            assert!(
                opening.contains(needle),
                "the first 512 characters must mention {needle}, clients truncate the rest"
            );
        }
    }

    #[test]
    fn the_theme_pointer_survives() {
        assert!(INSTRUCTIONS.contains("theme_get FIRST"));
    }

    /// The instructions sit in every client's context on every turn. Sediment
    /// settles here first; the budget is the brake.
    #[test]
    fn the_instructions_stay_inside_the_word_budget() {
        let words = INSTRUCTIONS.split_whitespace().count();
        assert!(
            words <= 250,
            "instructions grew to {words} words; move detail to its owner"
        );
    }

    #[test]
    fn the_design_prompt_carries_every_step_with_its_criterion() {
        let path = serde_json::json!([
            {"step": 1, "do": "Map the idea.", "done": "Every tile is named."},
            {"step": 2, "do": "Read the kit.", "done": "One cover chosen."}
        ]);
        let prompt = super::design_plugin_prompt(&path);
        assert!(prompt.contains("1. Map the idea. Done when: Every tile is named."));
        assert!(prompt.contains("2. Read the kit. Done when: One cover chosen."));
    }
}
