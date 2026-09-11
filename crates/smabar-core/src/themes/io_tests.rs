//! Tests for the shared theme-file I/O layer.

use serde_json::json;
use std::sync::{Arc, Barrier};

use crate::config::{BarPosition, SmabarConfig, SmabarPaths};

use super::io::*;
use super::settings::{ALLOWED_PATHS, activate};
use super::{ThemeDocument, ThemeMeta, bundled_default, read_dropin, resolve};

fn temp_paths() -> (tempfile::TempDir, SmabarPaths) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let paths = SmabarPaths::new(dir.path().join("smabar"));
    (dir, paths)
}

#[test]
fn saving_the_current_look_bakes_overrides_in_and_reactivates_as_a_noop() {
    let (_dir, paths) = temp_paths();
    let mut config = SmabarConfig::default();
    config.layout.position = BarPosition::Top;
    config
        .appearance
        .tokens
        .insert("--sb-accent".to_string(), "#00ff88".to_string());

    let _pending = stage_current_theme(&paths, &config, "my-look", false).expect("save");

    // Tokens: the active base resolved, with the override baked in.
    let mut expected = resolve(&paths, &config.theme);
    expected.insert("--sb-accent".to_string(), "#00ff88".to_string());
    assert_eq!(resolve(&paths, "my-look"), expected);

    // Settings: the complete live snapshot, so activating the saved theme
    // right away changes nothing (the analogue of
    // activating_the_default_theme_changes_nothing).
    let mut activated_input = config.clone();
    activated_input.theme = "my-look".to_string();
    activated_input.appearance.tokens.clear();
    let (activated, errors) = activate(&paths, activated_input.clone());
    assert!(errors.is_empty());
    assert_eq!(activated, activated_input);
}

#[test]
fn write_theme_rejects_bad_names_bundled_names_and_collisions() {
    let (_dir, paths) = temp_paths();
    let document = ThemeDocument::default();
    assert!(matches!(
        write_theme(&paths, "Bad Name", &document, false),
        Err(ThemeIoError::InvalidName(name)) if name == "Bad Name"
    ));
    assert!(matches!(
        write_theme(&paths, "default", &document, false),
        Err(ThemeIoError::BundledReadOnly(name)) if name == "default"
    ));
    write_theme(&paths, "mine", &document, false).expect("first write");
    assert!(matches!(
        write_theme(&paths, "mine", &document, false),
        Err(ThemeIoError::Exists(name)) if name == "mine"
    ));
    write_theme(&paths, "mine", &document, true).expect("overwrite");
    // Atomic write leaves no temp file behind.
    let leftovers: Vec<_> = std::fs::read_dir(paths.themes_dir())
        .expect("themes dir")
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn rollback_never_overwrites_a_newer_identical_write() {
    let (_dir, paths) = temp_paths();
    let old = parse_document_strict(r##"{"--sb-accent":"#123456"}"##).expect("old theme");
    let new = parse_document_strict(r##"{"--sb-accent":"#abcdef"}"##).expect("new theme");
    write_theme(&paths, "mine", &old, false).expect("initial write");
    let pending = stage_theme_write(&paths, "mine", &new, true).expect("staged write");

    write_theme(&paths, "mine", &new, true).expect("newer identical write");

    assert!(!pending.rollback().expect("rollback check"));
    assert_eq!(
        resolve(&paths, "mine")
            .get("--sb-accent")
            .map(String::as_str),
        Some("#abcdef")
    );
}

#[test]
fn delete_theme_removes_dropins_and_refuses_the_rest() {
    let (_dir, paths) = temp_paths();
    write_theme(&paths, "mine", &ThemeDocument::default(), false).expect("write");
    delete_theme(&paths, "mine").expect("delete");
    assert!(!paths.themes_dir().join("mine.json").exists());
    assert!(matches!(
        delete_theme(&paths, "mine"),
        Err(ThemeIoError::NotFound(message)) if message == "no drop-in theme \"mine\""
    ));
    assert!(matches!(
        delete_theme(&paths, "topbar"),
        Err(ThemeIoError::BundledReadOnly(name)) if name == "topbar"
    ));
}

#[test]
fn snapshot_settings_covers_every_allowed_path() {
    let snapshot = snapshot_settings(&SmabarConfig::default());
    assert_eq!(snapshot.len(), ALLOWED_PATHS.len());
    assert_eq!(snapshot.get("layout.position"), Some(&json!("bottom")));
}

#[test]
fn export_document_is_self_contained_even_for_partial_dropins() {
    let (dir, paths) = temp_paths();
    let document = parse_document_strict(
        r##"{"--sb-accent":"#00ff88","settings":{"layout.position":"top"},"meta":{"author":"Tester"}}"##,
    )
    .expect("valid document");
    write_theme(&paths, "partial", &document, false).expect("write");

    let exported_path =
        export_to_dir(&paths, "partial", &dir.path().join("exports")).expect("export");
    let exported = parse_document_strict(
        &std::fs::read_to_string(exported_path).expect("read exported theme"),
    )
    .expect("parse exported theme");
    // Tokens complete via the bundled-default fallback, override applied.
    assert_eq!(exported.tokens.len(), bundled_default().len());
    assert_eq!(
        exported.tokens.get("--sb-accent"),
        Some(&"#00ff88".to_string())
    );
    // Settings complete: default's canonical block overlaid with the theme's.
    assert_eq!(exported.settings.len(), ALLOWED_PATHS.len());
    assert_eq!(
        exported.settings.get("layout.position"),
        Some(&json!("top"))
    );
    // Metadata preserved.
    assert_eq!(exported.meta.author.as_deref(), Some("Tester"));

    assert!(matches!(
        export_to_dir(&paths, "missing", &dir.path().join("exports")),
        Err(ThemeIoError::NotFound(message)) if message == "no theme \"missing\""
    ));
}

#[test]
fn export_rejects_tolerated_settings_and_meta_instead_of_writing_an_unimportable_file() {
    let (dir, paths) = temp_paths();
    std::fs::create_dir_all(paths.themes_dir()).expect("themes dir");
    std::fs::write(
        paths.themes_dir().join("broken.json"),
        r##"{
            "--sb-accent": "#00ff88",
            "settings": { "language": "de" },
            "meta": { "author": "" }
        }"##,
    )
    .expect("write tolerated drop-in");

    let target = dir.path().join("exports");
    let errors = match export_to_dir(&paths, "broken", &target) {
        Err(ThemeIoError::InvalidDocument(errors)) => errors,
        other => panic!("expected strict export rejection, got {other:?}"),
    };
    assert!(errors.iter().any(|error| error.contains("\"language\"")));
    assert!(errors.iter().any(|error| error.contains("meta.author")));

    assert!(!target.exists(), "a rejected export must write nothing");
}

#[test]
fn export_to_dir_suffixes_name_collisions() {
    let (dir, paths) = temp_paths();
    let target = dir.path().join("exports");
    let first = export_to_dir(&paths, "default", &target).expect("first export");
    let second = export_to_dir(&paths, "default", &target).expect("second export");
    assert_eq!(first.file_name().unwrap(), "default.json");
    assert_eq!(second.file_name().unwrap(), "default-2.json");
}

#[test]
fn concurrent_same_name_writes_are_complete() {
    let (_dir, paths) = temp_paths();
    let paths = Arc::new(paths);
    let start = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for accent in ["#00ff88", "#ff0088"] {
        let paths = Arc::clone(&paths);
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            let document = parse_document_strict(&format!(r##"{{"--sb-accent":"{accent}"}}"##))
                .expect("valid document");
            start.wait();
            write_theme(&paths, "shared", &document, true)
        }));
    }
    start.wait();
    for handle in handles {
        handle.join().expect("writer thread").expect("write");
    }

    let raw =
        std::fs::read_to_string(paths.themes_dir().join("shared.json")).expect("complete theme");
    let document = parse_document_strict(&raw).expect("strictly valid theme");
    assert!(matches!(
        document.tokens.get("--sb-accent").map(String::as_str),
        Some("#00ff88" | "#ff0088")
    ));
    assert!(!paths.themes_dir().join("shared.json.tmp").exists());
}

#[test]
fn concurrent_no_overwrite_writes_have_one_winner() {
    let (_dir, paths) = temp_paths();
    let paths = Arc::new(paths);
    let start = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let paths = Arc::clone(&paths);
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            start.wait();
            write_theme(&paths, "shared", &ThemeDocument::default(), false)
        }));
    }
    start.wait();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("writer thread"))
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ThemeIoError::Exists(name)) if name == "shared"))
            .count(),
        1
    );
}

#[test]
fn concurrent_exports_get_distinct_importable_files() {
    let (dir, paths) = temp_paths();
    let paths = Arc::new(paths);
    let target = Arc::new(dir.path().join("exports"));
    let start = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let paths = Arc::clone(&paths);
        let target = Arc::clone(&target);
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            start.wait();
            export_to_dir(&paths, "default", &target)
        }));
    }
    start.wait();
    let mut files: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("export thread").expect("export"))
        .collect();
    files.sort();
    assert_eq!(
        files
            .iter()
            .map(|path| path.file_name().expect("file name"))
            .collect::<Vec<_>>(),
        ["default-2.json", "default.json"]
    );
    for file in files {
        let raw = std::fs::read_to_string(file).expect("read export");
        parse_document_strict(&raw).expect("export remains importable");
    }
}

#[test]
fn parse_document_strict_collects_every_problem_and_ignores_unknown_keys() {
    let errors = parse_document_strict(
        r##"{
            "--sb-accent": "#00ff88",
            "--sb-nope": "red",
            "settings": { "language": "de" },
            "meta": { "author": 5 }
        }"##,
    )
    .expect_err("three problems");
    assert_eq!(errors.len(), 3);
    assert!(errors.iter().any(|e| e.contains("--sb-nope")));
    assert!(errors.iter().any(|e| e.contains("\"language\"")));
    assert!(errors.iter().any(|e| e.contains("meta.author")));

    // Unknown non-token keys stay open for future format additions.
    let document = parse_document_strict(
        r##"{"$schema":"https://example.com/theme.json","future":{"x":1},"--sb-accent":"#00ff88"}"##,
    )
    .expect("forward compatible");
    assert_eq!(document.tokens.len(), 1);

    assert!(parse_document_strict("[1,2]").is_err());
}

#[test]
fn export_import_roundtrip_preserves_the_document() {
    let (dir, paths) = temp_paths();
    let document = parse_document_strict(
        r##"{"--sb-accent":"#00ff88","settings":{"layout.position":"top"},"meta":{"name":"Ocean","author":"Tester"}}"##,
    )
    .expect("valid document");
    write_theme(&paths, "ocean", &document, false).expect("write");

    let exported = export_to_dir(&paths, "ocean", &dir.path().join("exports")).expect("export");
    // Reimport into a fresh installation under a new stem.
    let renamed = dir.path().join("exports").join("My Ocean (1).json");
    std::fs::rename(&exported, &renamed).expect("rename");
    let (_dir2, paths2) = temp_paths();
    let name = import_theme_file(&paths2, &renamed, false).expect("import");
    assert_eq!(name, "my-ocean-1");
    assert_eq!(resolve(&paths2, &name), resolve(&paths, "ocean"));
    let meta = read_dropin(&paths2, &name).expect("dropin").meta;
    assert_eq!(meta.name.as_deref(), Some("Ocean"));
    assert_eq!(meta.author.as_deref(), Some("Tester"));
}

#[test]
fn import_rejects_bundled_stems_foreign_json_and_oversized_files() {
    let (dir, paths) = temp_paths();
    let write = |name: &str, content: &str| {
        let file = dir.path().join(name);
        std::fs::write(&file, content).expect("write input");
        file
    };

    let file = write("Default.json", r##"{"--sb-accent":"#00ff88"}"##);
    assert!(matches!(
        import_theme_file(&paths, &file, false),
        Err(ThemeIoError::BundledReadOnly(name)) if name == "default"
    ));

    // A random JSON object without tokens or settings is not a theme.
    let file = write("package.json", r##"{"nameField":"x","version":"1.0.0"}"##);
    assert!(matches!(
        import_theme_file(&paths, &file, false),
        Err(ThemeIoError::InvalidDocument(_))
    ));

    let file = write("broken.json", "{ not json");
    let error = import_theme_file(&paths, &file, false).expect_err("broken JSON");
    assert!(matches!(error, ThemeIoError::Parse { .. }));
    assert!(std::error::Error::source(&error).is_some());

    let file = write("huge.json", &"x".repeat((IMPORT_MAX_BYTES + 1) as usize));
    assert!(matches!(
        import_theme_file(&paths, &file, false),
        Err(ThemeIoError::InvalidDocument(_))
    ));

    assert!(matches!(
        import_theme_file(&paths, &dir.path().join("missing.json"), false),
        Err(ThemeIoError::Io { .. })
    ));

    // Collision flow: second import needs overwrite.
    let file = write("mine.json", r##"{"--sb-accent":"#00ff88"}"##);
    import_theme_file(&paths, &file, false).expect("first import");
    assert!(matches!(
        import_theme_file(&paths, &file, false),
        Err(ThemeIoError::Exists(name)) if name == "mine"
    ));
    import_theme_file(&paths, &file, true).expect("overwrite import");
}

#[test]
fn slugify_reduces_arbitrary_input_to_valid_names() {
    assert_eq!(slugify_theme_name("ocean"), Some("ocean".to_string()));
    assert_eq!(
        slugify_theme_name("My Theme (1)"),
        Some("my-theme-1".to_string())
    );
    assert_eq!(slugify_theme_name("__Ätna__"), Some("tna".to_string()));
    assert_eq!(slugify_theme_name("(!)"), None);
    assert_eq!(slugify_theme_name(""), None);
    let long = slugify_theme_name(&"a-".repeat(100)).expect("capped");
    assert!(long.len() <= 64);
    assert!(!long.ends_with('-'));
    assert!(super::is_valid_theme_name(&long));
}

#[test]
fn parse_meta_value_validates_fields_and_ignores_unknown_ones() {
    let meta = parse_meta_value(json!({
        "name": "Ocean",
        "author": "Tester",
        "futureField": { "ignored": true }
    }))
    .expect("valid meta");
    assert_eq!(meta.name.as_deref(), Some("Ocean"));
    assert_eq!(meta.version, None);

    assert!(parse_meta_value(json!("author: x")).is_err());
    assert!(parse_meta_value(json!({ "description": "x".repeat(300) })).is_err());
    assert!(parse_meta_value(json!({ "author": "" })).is_err());
    assert!(ThemeMeta::default().is_empty());
}
