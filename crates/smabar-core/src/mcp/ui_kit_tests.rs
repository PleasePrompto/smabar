//! Behavior tests for the `ui_kit` MCP tool + contract sanity.

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

use crate::themes;

use super::tests::{test_handler, unwrap_json};
use super::types::{UiKitParams, UiKitResult};

/// The class categories the tool exposes as sections.
const CATEGORIES: &[&str] = &[
    "actions",
    "content",
    "data",
    "disclosure",
    "feedback",
    "forms",
    "layout",
    "overlays",
];

fn params(section: Option<&str>) -> Parameters<UiKitParams> {
    Parameters(UiKitParams {
        sections: section.map(|section| vec![section.to_string()]),
        classes: None,
    })
}

#[test]
fn bundled_contract_parses_and_has_the_expected_shape() {
    let contract: Value = serde_json::from_str(include_str!("../../../../ui-kit/contract.json"))
        .expect("contract.json must be valid JSON");
    assert_eq!(contract["version"], 1);
    for key in [
        "designRules",
        "tileSizing",
        "classes",
        "classCategories",
        "icons",
        "iconsUsage",
        "charts",
        "sanitizer",
        "media",
        "formContract",
        "branding",
        "conventions",
        "renderTargets",
        "tileChrome",
        "snippets",
        "coverLayouts",
    ] {
        assert!(contract.get(key).is_some(), "contract misses {key}");
    }
    // The coverLayouts section's own shape lives in ui_kit_cover_tests.rs.
    let rules = contract["designRules"].as_array().expect("designRules");
    assert!(!rules.is_empty());
    assert!(
        rules.iter().any(|rule| rule
            .as_str()
            .is_some_and(|r| r.contains("desktop utility panels"))),
        "designRules must teach compact desktop panels"
    );
    // The old "set an inline min-width" advice is what made pills wider than
    // their content; tileSizing replaced it with per-content techniques.
    assert!(
        rules
            .iter()
            .all(|rule| !rule.as_str().unwrap_or_default().contains("min-width")),
        "designRules must not tell plugins to pad their tile out"
    );
    let scenarios = contract["tileSizing"]["scenarios"]
        .as_array()
        .expect("tileSizing.scenarios");
    assert!(
        scenarios.len() >= 4,
        "tileSizing must cover the common tile contents"
    );
    assert!(
        contract["branding"]["inline"]
            .as_str()
            .is_some_and(|text| text.contains("--sb-on-accent")),
        "branding must document how to tint an accent surface"
    );
    for target in ["tile", "flyout", "hover", "popup"] {
        assert!(
            contract["renderTargets"][target].is_string(),
            "renderTargets misses {target}"
        );
    }
    let classes = contract["classes"].as_array().expect("classes array");
    assert!(!classes.is_empty());
    for class in classes {
        // `category` is what the per-category sections slice on; an entry
        // without one would be unreachable through them.
        for field in ["name", "purpose", "example", "category"] {
            assert!(
                class[field].is_string(),
                "class entry misses {field}: {class}"
            );
        }
        assert!(
            CATEGORIES.contains(&class["category"].as_str().unwrap_or_default()),
            "class entry has an unknown category: {class}"
        );
    }
    assert!(
        !contract["icons"]
            .as_array()
            .expect("icons array")
            .is_empty()
    );
    for snippet in [
        // The three native-interaction patterns an agent cannot guess, plus
        // the table it would otherwise hand-roll.
        "accordion",
        "modal",
        "popoverMenu",
        "dataTable",
        "cardFlyout",
        "contextMenu",
        "form",
        "heroTabs",
        "heroTint",
        "kpiGrid",
        "mediaCarousel",
        "tabbedFlyout",
        "videoCard",
    ] {
        assert!(
            contract["snippets"][snippet].is_string(),
            "snippet {snippet} missing"
        );
    }
}

/// Custom right-click menus and tooltips are plugin-facing conventions, so
/// the contract the `ui_kit` tool serves must spell both out completely — a
/// fresh agent has no other source for them.
#[test]
fn contract_documents_context_menus_and_tooltips() {
    let contract: Value = serde_json::from_str(include_str!("../../../../ui-kit/contract.json"))
        .expect("contract.json must be valid JSON");
    let menu = &contract["conventions"]["contextMenu"];
    assert_eq!(menu["attribute"], "data-context-items");
    let usage = menu["usage"].as_str().expect("contextMenu usage");
    for word in [
        "label",
        "action",
        "value",
        "icon",
        "danger",
        "disabled",
        "checked",
        "separator",
        "items",
        "plugin_action",
    ] {
        assert!(usage.contains(word), "contextMenu usage misses {word}");
    }
    assert!(menu["example"].as_str().is_some_and(|example| {
        example.contains("data-context-items") && example.contains("\"items\"")
    }));

    let tooltip = &contract["conventions"]["tooltip"];
    assert_eq!(tooltip["attribute"], "title");
    assert!(
        tooltip["usage"]
            .as_str()
            .is_some_and(|usage| usage.contains("aria-label")),
        "tooltip usage must keep the a11y rule"
    );
}

#[tokio::test]
async fn all_returns_the_complete_contract_with_live_tokens_but_no_token_contract() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.ui_kit(params(Some("all"))).await).expect("ui_kit");
    assert_eq!(result.version, 1);
    assert_eq!(result.tokens.as_ref(), Some(themes::bundled_default()));
    // The markup contract points to the bounded theme contract lookup.
    assert!(
        result
            .token_contract
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|pointer| pointer.contains("theme_get"))
    );
    assert!(result.sections.is_none(), "all is not the index");
    for (name, section) in [
        ("designRules", &result.design_rules),
        ("bestPractices", &result.best_practices),
        ("classes", &result.classes),
        ("classCategories", &result.class_categories),
        ("icons", &result.icons),
        ("charts", &result.charts),
        ("sanitizer", &result.sanitizer),
        ("media", &result.media),
        ("formContract", &result.form_contract),
        ("branding", &result.branding),
        ("conventions", &result.conventions),
        ("renderTargets", &result.render_targets),
        ("tileChrome", &result.tile_chrome),
        ("snippets", &result.snippets),
    ] {
        assert!(section.is_some(), "section {name} missing in `all`");
    }
}

#[tokio::test]
async fn ui_kit_sections_return_only_their_slice() {
    let (_dir, mcp) = test_handler().await;

    let tokens = unwrap_json(mcp.ui_kit(params(Some("tokens"))).await).expect("tokens");
    assert!(tokens.tokens.is_some());
    assert!(tokens.token_contract.as_ref().is_some_and(Value::is_string));
    assert!(tokens.classes.is_none() && tokens.snippets.is_none());
    // The design rules lead EVERY reply, whatever section was requested — and
    // so does tileSizing, because rule 4 refers to it by name. It used to sit
    // in the contract file while the tool never served it, so the reference
    // pointed at nothing an agent could read.
    assert!(tokens.design_rules.is_some());
    assert!(tokens.tile_sizing.is_some());

    let classes = unwrap_json(mcp.ui_kit(params(Some("classes"))).await).expect("classes");
    assert!(classes.classes.is_some());
    assert!(classes.tokens.is_none() && classes.icons.is_none());

    // Icons carry the chart conventions along.
    let icons = unwrap_json(mcp.ui_kit(params(Some("icons"))).await).expect("icons");
    assert!(icons.icons.is_some() && icons.charts.is_some());
    assert!(icons.sanitizer.is_none());

    // Sanitizer carries media + form contract + branding along.
    let sanitizer = unwrap_json(mcp.ui_kit(params(Some("sanitizer"))).await).expect("sanitizer");
    assert!(sanitizer.sanitizer.is_some() && sanitizer.media.is_some());
    assert!(sanitizer.form_contract.is_some() && sanitizer.branding.is_some());
    assert!(sanitizer.icons.is_none());

    let conventions =
        unwrap_json(mcp.ui_kit(params(Some("conventions"))).await).expect("conventions");
    assert!(conventions.conventions.is_some());
    assert!(conventions.render_targets.is_some() && conventions.tile_chrome.is_some());
    assert!(conventions.sanitizer.is_none() && conventions.tokens.is_none());

    let snippets = unwrap_json(mcp.ui_kit(params(Some("snippets"))).await).expect("snippets");
    assert!(snippets.snippets.is_some());
    assert!(snippets.classes.is_none() && snippets.tokens.is_none());
}

#[tokio::test]
async fn ui_kit_rejects_unknown_sections() {
    let (_dir, mcp) = test_handler().await;
    let err = unwrap_json(mcp.ui_kit(params(Some("colors"))).await).expect_err("unknown section");
    assert!(err.message.contains("colors"));
    assert!(err.message.contains("snippets"));
    // The categories are sections too, so the error has to list them — an
    // agent that guessed "colors" has no other way to learn "content".
    for category in CATEGORIES {
        assert!(
            err.message.contains(category),
            "the error must name the {category} category"
        );
    }
}

/// The harvested component vocabulary is generated, so nothing hand-checks
/// it: this is the only place its shape is guaranteed.
#[test]
fn the_generated_kit_classes_are_complete_and_categorised() {
    let generated: Value =
        serde_json::from_str(include_str!("../../../../ui-kit/kit-classes.json"))
            .expect("kit-classes.json must be valid JSON");
    let classes = generated["classes"].as_array().expect("classes array");
    assert!(
        classes.len() > 100,
        "the harvest brought 50+ components across; got {}",
        classes.len()
    );
    let mut seen = std::collections::HashSet::new();
    for class in classes {
        for field in ["name", "purpose", "example", "category"] {
            assert!(class[field].is_string(), "entry misses {field}: {class}");
            assert!(
                !class[field].as_str().unwrap_or_default().is_empty(),
                "entry has an empty {field}: {class}"
            );
        }
        let name = class["name"].as_str().unwrap_or_default();
        assert!(
            CATEGORIES.contains(&class["category"].as_str().unwrap_or_default()),
            "{name} has an unknown category"
        );
        assert!(
            class["example"].as_str().unwrap_or_default().contains(name),
            "{name}'s example never shows the class"
        );
        assert!(seen.insert(name), "{name} is documented twice");
    }
    // Every category has to carry something, or its section answers nothing.
    for category in CATEGORIES {
        assert!(
            classes.iter().any(|class| class["category"] == *category) || contract_has(category),
            "no class in category {category}"
        );
    }
}

/// Whether the hand-written contract carries a class of that category.
fn contract_has(category: &str) -> bool {
    let contract: Value = serde_json::from_str(include_str!("../../../../ui-kit/contract.json"))
        .expect("contract.json must be valid JSON");
    contract["classes"]
        .as_array()
        .is_some_and(|classes| classes.iter().any(|class| class["category"] == category))
}

/// The default reply must stay readable: the harvested components more than
/// double it, so `all` hands over the core vocabulary and the index to the
/// rest, and `classes` is the explicit "everything".
#[tokio::test]
async fn all_carries_the_core_classes_and_the_category_index() {
    let (_dir, mcp) = test_handler().await;
    let default = unwrap_json(mcp.ui_kit(params(Some("all"))).await).expect("all");
    let every = unwrap_json(mcp.ui_kit(params(Some("classes"))).await).expect("classes");
    let count = |result: &UiKitResult| {
        result
            .classes
            .as_ref()
            .and_then(Value::as_array)
            .expect("classes")
            .len()
    };
    assert!(
        count(&every) > count(&default),
        "section classes must add the harvested vocabulary"
    );
    // Whatever is left out has to be findable, so the index travels with
    // every reply that carries classes at all.
    assert!(default.class_categories.is_some());
    assert!(every.class_categories.is_some());
    for category in CATEGORIES {
        assert!(
            default.class_categories.as_ref().expect("index")[category].is_string(),
            "the index misses {category}"
        );
    }
}

#[tokio::test]
async fn ui_kit_category_sections_return_only_their_own_classes() {
    let (_dir, mcp) = test_handler().await;
    let all = unwrap_json(mcp.ui_kit(params(Some("classes"))).await).expect("classes");
    let total = all
        .classes
        .as_ref()
        .and_then(Value::as_array)
        .expect("all")
        .len();

    let mut summed = 0;
    for category in CATEGORIES {
        let result = unwrap_json(mcp.ui_kit(params(Some(category))).await).expect("category");
        let classes = result
            .classes
            .as_ref()
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{category} returns classes"));
        assert!(!classes.is_empty(), "{category} is empty");
        for class in classes {
            assert_eq!(class["category"], *category, "{category} leaked a class");
        }
        summed += classes.len();
        // A category is a NARROWING: everything else stays out, but the
        // rules that lead every reply still lead this one.
        assert!(result.tokens.is_none(), "{category} carries the theme");
        assert!(result.icons.is_none(), "{category} carries the icons");
        assert!(result.snippets.is_none(), "{category} carries the snippets");
        assert!(
            result.sanitizer.is_none(),
            "{category} carries the sanitizer"
        );
        assert!(result.design_rules.is_some() && result.tile_sizing.is_some());
        assert!(
            result.class_categories.is_some(),
            "{category} lost the index"
        );
    }
    assert_eq!(
        summed, total,
        "the categories must partition the class list"
    );
}

/// A form built from the form classes alone never reaches the plugin: the
/// values travel through `data-field`, which only formContract explains.
#[tokio::test]
async fn ui_kit_forms_section_carries_the_form_contract() {
    let (_dir, mcp) = test_handler().await;
    let forms = unwrap_json(mcp.ui_kit(params(Some("forms"))).await).expect("forms");
    assert!(forms.form_contract.is_some());
    let data = unwrap_json(mcp.ui_kit(params(Some("data"))).await).expect("data");
    assert!(data.form_contract.is_none());
}

#[tokio::test]
async fn ui_kit_tokens_follow_the_active_theme() {
    let (_dir, mcp) = test_handler().await;
    // Write + activate a drop-in theme, then the tool must serve it.
    let dir = mcp.paths.themes_dir();
    std::fs::create_dir_all(&dir).expect("create themes dir");
    std::fs::write(dir.join("neon.json"), r##"{"--sb-accent":"#00ff88"}"##).expect("write theme");
    let mut config = mcp.config.current();
    config.theme = "neon".to_string();
    mcp.config.apply(config).expect("activate theme");

    let result = unwrap_json(mcp.ui_kit(params(Some("tokens"))).await).expect("tokens");
    let tokens = result.tokens.expect("tokens present");
    assert_eq!(
        tokens.get("--sb-accent").map(String::as_str),
        Some("#00ff88")
    );
}

#[tokio::test]
async fn every_reply_carries_the_behaviour_index() {
    // An agent that never learns the hooks exist will tell the user the bar
    // cannot do it. The index is small enough to ride along on every reply,
    // so no section may drop it.
    let (_dir, mcp) = test_handler().await;
    for section in [None, Some("classes"), Some("forms"), Some("snippets")] {
        let result = unwrap_json(mcp.ui_kit(params(section)).await).expect("ui_kit");
        let behaviour = result
            .behaviour
            .unwrap_or_else(|| panic!("section {section:?} dropped the behaviour index"));
        let hooks = behaviour["hooks"].as_array().expect("hooks list");
        assert!(hooks.len() >= 15, "only {} hooks indexed", hooks.len());
        // The index has to name the way to the full markup, or it is a dead
        // end: the agent knows the hook exists and not how to write it.
        assert!(
            behaviour["fullExamples"]
                .as_str()
                .unwrap_or_default()
                .contains("behaviour")
        );
        assert!(behaviour["stateNote"].is_string());
    }
}

#[tokio::test]
async fn the_behaviour_section_serves_markup_for_every_hook() {
    let (_dir, mcp) = test_handler().await;
    let result = unwrap_json(mcp.ui_kit(params(Some("behaviour"))).await).expect("behaviour");
    let hooks = result.behaviour.expect("behaviour section")["hooks"]
        .as_array()
        .expect("hooks list")
        .clone();
    for entry in hooks {
        let hook = entry["hook"].as_str().expect("hook name");
        let example = entry["example"].as_str().expect("example markup");
        assert!(
            example.contains(hook),
            "{hook}: its example does not use the hook"
        );
        assert!(
            entry["what"].as_str().is_some_and(|what| what.len() > 40),
            "{hook}: no explanation of what it does"
        );
    }
}

#[tokio::test]
async fn the_tool_description_points_at_the_behaviour_section() {
    // A fresh session knows only the tool descriptions. If they still say
    // "no JavaScript" without naming what replaces it, the capability is
    // invisible.
    let described = super::SmabarMcp::ui_kit_tool_router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "ui_kit")
        .and_then(|tool| tool.description)
        .expect("ui_kit is described")
        .to_string();
    assert!(described.contains("behaviour"), "{described}");
    assert!(described.contains("data-sb-"), "{described}");
    // Same for the tile covers: undescribed, the vocabulary is invisible.
    assert!(described.contains("coverLayouts"), "{described}");
}
