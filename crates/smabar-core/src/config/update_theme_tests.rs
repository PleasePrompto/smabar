use serde_json::json;

use super::super::{BarPosition, SmabarConfig, SmabarPaths};
use super::{SetPathError, set_config_path_activating};

#[test]
fn activating_a_theme_path_applies_its_settings_block() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    std::fs::create_dir_all(paths.themes_dir()).expect("create themes dir");
    std::fs::write(
        paths.themes_dir().join("full.json"),
        r##"{"--sb-accent":"#00ff88","settings":{"layout.position":"top","language":"de"}}"##,
    )
    .expect("write theme");

    let config = SmabarConfig::default();
    let (updated, errors) =
        set_config_path_activating(&paths, &config, "theme", json!("full")).expect("activate");
    assert_eq!(updated.theme, "full");
    assert_eq!(updated.layout.position, BarPosition::Top);
    assert_eq!(updated.language, "en");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("language"));

    let (back, errors) = set_config_path_activating(&paths, &updated, "theme", json!("default"))
        .expect("back to default");
    assert!(errors.is_empty());
    assert_eq!(back, SmabarConfig::default());

    let (plain, errors) =
        set_config_path_activating(&paths, &config, "layout.position", json!("top"))
            .expect("plain set");
    assert!(errors.is_empty());
    assert_eq!(plain.layout.position, BarPosition::Top);
}

#[test]
fn activating_an_uninstalled_theme_is_rejected() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    let error =
        set_config_path_activating(&paths, &SmabarConfig::default(), "theme", json!("missing"))
            .expect_err("missing theme");
    assert!(matches!(error, SetPathError::UnknownTheme { name } if name == "missing"));
}
