//! Behavior tests for the shortcuts service against tempdir fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{ShortcutEntry, SmabarConfig};

use super::icons::IconDirs;
use super::{ShortcutError, ShortcutsService, pin_shortcut, unpin_shortcut};

/// A 1x1 transparent PNG.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    fs::write(path, content).expect("write fixture");
}

/// Service over one apps dir + one pixmaps dir inside a tempdir.
fn fixture() -> (tempfile::TempDir, ShortcutsService) {
    let dir = tempfile::tempdir().expect("tempdir");
    let apps = dir.path().join("applications");
    write(
        &apps.join("calc.desktop"),
        "[Desktop Entry]\nType=Application\nName=Calculator\nComment=Do math\n\
         Exec=cargo --version %u\nIcon=calc\n",
    );
    write(
        &apps.join("editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nComment=Write text\nExec=cargo --version\n",
    );
    let pixmaps = dir.path().join("pixmaps");
    fs::create_dir_all(&pixmaps).expect("create pixmaps");
    fs::write(pixmaps.join("calc.png"), TINY_PNG).expect("write icon");
    let platform = crate::platform::shortcuts::xdg(
        vec![apps],
        IconDirs {
            theme_roots: Vec::new(),
            flat_dirs: vec![pixmaps],
            ..IconDirs::default()
        },
        None,
        dir.path().to_path_buf(),
    );
    let service = ShortcutsService::new(platform, dir.path().join("icons-cache"));
    (dir, service)
}

#[test]
fn search_matches_name_and_comment_case_insensitively() {
    let (_dir, service) = fixture();
    let names = |query: &str| -> Vec<String> {
        service
            .search(query)
            .expect("search apps")
            .into_iter()
            .map(|a| a.name)
            .collect()
    };
    assert_eq!(names(""), vec!["Calculator", "Editor"]);
    assert_eq!(names("CALC"), vec!["Calculator"]);
    assert_eq!(names("write text"), vec!["Editor"]);
    assert_eq!(names("no such app"), Vec::<String>::new());
}

#[test]
fn refresh_picks_up_newly_installed_apps() {
    let (dir, service) = fixture();
    assert_eq!(service.search("browser").expect("first search").len(), 0);
    write(
        &dir.path().join("applications/browser.desktop"),
        "[Desktop Entry]\nType=Application\nName=Browser\nExec=true\n",
    );
    // The scan is cached until a refresh.
    assert_eq!(service.search("browser").expect("cached search").len(), 0);
    service.refresh();
    assert_eq!(
        service.search("browser").expect("refreshed search").len(),
        1
    );
}

#[test]
fn validated_entry_checks_the_source_and_generates_stable_ids() {
    let (dir, service) = fixture();

    let entry = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, false)
        .expect("pin by desktop id");
    assert_eq!(entry.desktop_id.as_deref(), Some("calc.desktop"));
    assert!(entry.id.starts_with("sc-"), "id: {}", entry.id);
    let again = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, false)
        .expect("same source");
    assert_eq!(entry.id, again.id, "same source must yield the same id");

    let err = service
        .validated_entry(Some("ghost.desktop".to_string()), None, None, None, false)
        .expect_err("unknown desktop id");
    assert!(matches!(err, ShortcutError::UnknownDesktopId { .. }));

    let err = service
        .validated_entry(None, None, None, None, false)
        .expect_err("no source");
    assert!(matches!(err, ShortcutError::InvalidSource));
    let err = service
        .validated_entry(
            Some("calc.desktop".to_string()),
            Some(PathBuf::from("/x.desktop")),
            None,
            None,
            false,
        )
        .expect_err("both sources");
    assert!(matches!(err, ShortcutError::InvalidSource));

    for bad in [
        dir.path().join("applications/missing.desktop"),
        dir.path().join("applications/calc.txt"),
        PathBuf::from("relative.desktop"),
    ] {
        let err = service
            .validated_entry(None, Some(bad.clone()), None, None, false)
            .expect_err("invalid path");
        assert!(
            matches!(err, ShortcutError::InvalidDesktopPath { .. }),
            "path {bad:?}"
        );
    }

    let pinned_path = dir.path().join("applications/editor.desktop");
    let entry = service
        .validated_entry(
            None,
            Some(pinned_path.clone()),
            None,
            Some("My Editor".to_string()),
            false,
        )
        .expect("pin by path");
    assert_eq!(entry.path.as_deref(), Some(pinned_path.as_path()));
    assert_eq!(entry.label.as_deref(), Some("My Editor"));
}

#[test]
fn pin_and_unpin_are_pure_and_reject_duplicates_and_unknown_ids() {
    let (_dir, service) = fixture();
    let config = SmabarConfig::default();
    let calc = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, false)
        .expect("entry");
    let editor = service
        .validated_entry(Some("editor.desktop".to_string()), None, None, None, false)
        .expect("entry");

    let one = pin_shortcut(&config, calc.clone(), None).expect("pin calc");
    assert!(config.shortcuts.pinned.is_empty(), "input stays untouched");
    // Index 0 inserts in front; out-of-range indexes clamp to append.
    let two = pin_shortcut(&one, editor.clone(), Some(0)).expect("pin editor first");
    let ids: Vec<&str> = two.shortcuts.pinned.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec![editor.id.as_str(), calc.id.as_str()]);

    let err = pin_shortcut(&two, calc.clone(), None).expect_err("duplicate");
    assert!(matches!(err, ShortcutError::AlreadyPinned { .. }));

    let without = unpin_shortcut(&two, &calc.id).expect("unpin");
    assert_eq!(without.shortcuts.pinned.len(), 1);
    let err = unpin_shortcut(&without, &calc.id).expect_err("already gone");
    assert!(matches!(err, ShortcutError::UnknownPin { .. }));
}

#[test]
fn separators_are_unique_resolved_removable_and_never_launchable() {
    let (_dir, service) = fixture();
    let first = service
        .validated_entry(None, None, None, None, true)
        .expect("first separator");
    let second = service
        .validated_entry(None, None, None, None, true)
        .expect("second separator");
    assert!(first.separator);
    assert_ne!(first.id, second.id);
    assert!(first.desktop_id.is_none() && first.path.is_none());

    let err = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, true)
        .expect_err("separator with launch source");
    assert!(matches!(err, ShortcutError::InvalidSource));

    let config = pin_shortcut(&SmabarConfig::default(), first.clone(), None).expect("pin first");
    let config = pin_shortcut(&config, second, None).expect("pin second");
    let resolved = service.resolve_pinned(&config.shortcuts);
    assert_eq!(resolved.len(), 2);
    assert!(resolved.iter().all(|entry| {
        entry.separator
            && entry.label.is_empty()
            && entry.icons.is_empty()
            && entry.desktop_id.is_none()
    }));

    let err = service.launch_pinned(&first).expect_err("launch separator");
    assert!(matches!(err, ShortcutError::SeparatorNotLaunchable { .. }));
    let config = unpin_shortcut(&config, &first.id).expect("unpin separator");
    assert_eq!(config.shortcuts.pinned.len(), 1);
}

#[test]
fn resolve_pinned_inlines_icons_and_keeps_broken_pins_visible() {
    let (_dir, service) = fixture();
    let config = SmabarConfig::default();
    let calc = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, false)
        .expect("entry");
    let mut config = pin_shortcut(&config, calc, None).expect("pin");
    // A pin whose app was uninstalled since (never validated against scan).
    config.shortcuts.pinned.push(ShortcutEntry {
        id: "sc-gone0000".to_string(),
        desktop_id: Some("gone.desktop".to_string()),
        ..ShortcutEntry::default()
    });

    let resolved = service.resolve_pinned(&config.shortcuts);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].label, "Calculator");
    let icon = resolved[0].icons.first().expect("icon inlined").as_str();
    assert!(icon.starts_with("data:image/png;base64,"), "icon: {icon}");
    // The broken pin stays listed so it can be unpinned.
    assert_eq!(resolved[1].label, "gone");
    assert!(resolved[1].icons.is_empty());
}

#[test]
fn oversized_icons_are_not_inlined() {
    let (dir, service) = fixture();
    fs::write(dir.path().join("pixmaps/editor.png"), vec![0u8; 513 * 1024])
        .expect("write big icon");
    write(
        &dir.path().join("applications/big.desktop"),
        "[Desktop Entry]\nType=Application\nName=Big\nExec=true\nIcon=editor\n",
    );
    service.refresh();
    assert_eq!(service.app_icon_data_uri("big.desktop"), None);
    // The small icon still resolves.
    assert!(service.app_icon_data_uri("calc.desktop").is_some());
}

#[test]
fn file_and_folder_pins_validate_resolve_and_build_the_opener_argv() {
    let dir = tempfile::tempdir().expect("tempdir");
    let music = dir.path().join("Music");
    fs::create_dir(&music).expect("create folder");
    // A theme with the generic file and folder icons used by path pins.
    let themes = dir.path().join("icons");
    write(
        &themes.join("hicolor/64x64/places/folder.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    );
    write(
        &themes.join("hicolor/64x64/mimetypes/text-x-generic.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    );
    let platform = crate::platform::shortcuts::xdg(
        vec![dir.path().join("applications")],
        IconDirs {
            theme_roots: vec![themes],
            ..IconDirs::default()
        },
        None,
        dir.path().to_path_buf(),
    );
    let service = ShortcutsService::new(platform, dir.path().join("icons-cache"));

    let entry = service
        .validated_entry(None, Some(music.clone()), None, None, false)
        .expect("pin a folder");
    assert_eq!(entry.path.as_deref(), Some(music.as_path()));
    assert!(entry.id.starts_with("sc-"), "id: {}", entry.id);
    let notes = dir.path().join("release notes.pdf");
    fs::write(&notes, b"not really a pdf").expect("write file");
    let file = service
        .validated_entry(None, Some(notes.clone()), None, None, false)
        .expect("pin a file");
    assert_eq!(file.path.as_deref(), Some(notes.as_path()));

    for bad in [PathBuf::from("Music"), dir.path().join("missing-dir")] {
        let err = service
            .validated_entry(None, Some(bad.clone()), None, None, false)
            .expect_err("invalid folder path");
        assert!(
            matches!(err, ShortcutError::InvalidDesktopPath { .. }),
            "path {bad:?}"
        );
    }

    // Label defaults to the directory name; a stored label wins. The icon
    // is the theme's `folder` icon from the `places` context.
    let mut config = pin_shortcut(&SmabarConfig::default(), entry, None).expect("pin");
    config.shortcuts.pinned.push(ShortcutEntry {
        label: Some("Tunes".to_string()),
        ..service
            .validated_entry(None, Some(dir.path().to_path_buf()), None, None, false)
            .expect("pin second folder")
    });
    config.shortcuts.pinned.push(file);
    let resolved = service.resolve_pinned(&config.shortcuts);
    assert_eq!(resolved[0].label, "Music");
    assert_eq!(resolved[1].label, "Tunes");
    assert_eq!(resolved[2].label, "release notes.pdf");
    assert!(!resolved[2].icons.is_empty(), "generic file icon");
    let icon = resolved[0].icons.first().expect("folder icon").as_str();
    assert!(
        icon.starts_with("data:image/svg+xml;base64,"),
        "icon: {icon}"
    );
}

#[test]
fn vanished_folder_stays_a_visible_broken_pin_and_refuses_to_launch() {
    let (dir, service) = fixture();
    let gone = dir.path().join("Downloads");
    fs::create_dir(&gone).expect("create folder");
    let entry = service
        .validated_entry(None, Some(gone.clone()), None, None, false)
        .expect("pin folder");
    fs::remove_dir(&gone).expect("remove folder");

    let config = pin_shortcut(&SmabarConfig::default(), entry.clone(), None).expect("pin");
    let resolved = service.resolve_pinned(&config.shortcuts);
    assert_eq!(resolved[0].label, "Downloads");
    assert!(resolved[0].icons.is_empty());

    let err = service.launch_pinned(&entry).expect_err("vanished folder");
    assert!(matches!(err, ShortcutError::InvalidDesktopPath { .. }));
}

#[test]
fn launch_pinned_spawns_from_the_parsed_desktop_file_only() {
    let (dir, service) = fixture();
    let calc = service
        .validated_entry(Some("calc.desktop".to_string()), None, None, None, false)
        .expect("entry");
    service.launch_pinned(&calc).expect("launch by desktop id");

    let by_path = service
        .validated_entry(
            None,
            Some(dir.path().join("applications/editor.desktop")),
            None,
            None,
            false,
        )
        .expect("entry");
    service.launch_pinned(&by_path).expect("launch by path");

    // A terminal app pinned by explicit path is refused, not mangled.
    let term = dir.path().join("applications/term.desktop");
    write(
        &term,
        "[Desktop Entry]\nType=Application\nName=Term\nExec=true\nTerminal=true\n",
    );
    let entry = ShortcutEntry {
        id: "sc-term0000".to_string(),
        path: Some(term),
        ..ShortcutEntry::default()
    };
    let err = service.launch_pinned(&entry).expect_err("terminal app");
    assert!(matches!(err, ShortcutError::TerminalApp { .. }));

    // An empty Exec after field-code stripping is an error, not a spawn.
    let codes = dir.path().join("applications/codes.desktop");
    write(
        &codes,
        "[Desktop Entry]\nType=Application\nName=Codes\nExec=%U\n",
    );
    let entry = ShortcutEntry {
        id: "sc-codes000".to_string(),
        path: Some(codes),
        ..ShortcutEntry::default()
    };
    let err = service.launch_pinned(&entry).expect_err("empty exec");
    assert!(matches!(err, ShortcutError::EmptyExec { .. }));
}

#[test]
fn website_pins_validate_resolve_offline_and_reject_bad_urls() {
    let (_dir, service) = fixture();
    let bild = "https://www.bild.de/";
    let entry = service
        .validated_entry(None, None, Some(bild.to_string()), None, false)
        .expect("pin a website");
    assert_eq!(entry.url.as_deref(), Some(bild));
    assert!(entry.id.starts_with("sc-"), "id: {}", entry.id);
    let again = service
        .validated_entry(None, None, Some(bild.to_string()), None, false)
        .expect("same url");
    assert_eq!(entry.id, again.id, "same url must yield the same id");

    for bad in [
        "javascript:alert(1)",
        "example.com",
        "https://",
        "file:///etc",
    ] {
        let err = service
            .validated_entry(None, None, Some(bad.to_string()), None, false)
            .expect_err("invalid url");
        assert!(matches!(err, ShortcutError::InvalidUrl(_)), "url {bad}");
    }
    let err = service
        .validated_entry(
            Some("calc.desktop".to_string()),
            None,
            Some(bild.to_string()),
            None,
            false,
        )
        .expect_err("two sources");
    assert!(matches!(err, ShortcutError::InvalidSource));

    // Resolution is offline: label and favicon both come from the URL.
    let mut config = pin_shortcut(&SmabarConfig::default(), entry, None).expect("pin");
    config.shortcuts.pinned.push(
        service
            .validated_entry(
                None,
                None,
                Some("https://www.spiegel.de/politik".to_string()),
                Some("Nachrichten".to_string()),
                false,
            )
            .expect("second website"),
    );
    let resolved = service.resolve_pinned(&config.shortcuts);
    assert_eq!(resolved[0].label, "bild.de");
    assert_eq!(
        resolved[0].icons,
        vec![
            "https://www.bild.de/apple-touch-icon.png",
            "https://www.bild.de/favicon.ico",
        ]
    );
    assert!(!resolved[0].separator && resolved[0].desktop_id.is_none());
    assert_eq!(resolved[1].label, "Nachrichten");
}

#[test]
fn a_downloaded_website_icon_is_offered_before_the_remote_candidates() {
    // The reason the fix exists: the first remote candidate is
    // /apple-touch-icon.png, and a site without one answers with its regular
    // 404 PAGE. The webview downloads that whole document before it can move
    // on — a visibly broken tile on every cold start. A cached file skips all
    // of it and works offline.
    let (_dir, service) = fixture();
    let entry = service
        .validated_entry(
            None,
            None,
            Some("https://example.com/".to_string()),
            None,
            false,
        )
        .expect("pin a website");
    let config = pin_shortcut(&SmabarConfig::default(), entry, None).expect("pin");

    let before = service.resolve_pinned(&config.shortcuts);
    assert_eq!(
        before[0].icons[0], "https://example.com/apple-touch-icon.png",
        "without a cache the remote candidates lead"
    );

    fs::create_dir_all(service.icons_dir()).expect("create icon cache");
    fs::write(service.icons_dir().join("example.com.png"), TINY_PNG).expect("cache an icon");

    let after = service.resolve_pinned(&config.shortcuts);
    assert!(
        after[0].icons[0].starts_with("data:image/png;base64,"),
        "the cached file must lead, got {}",
        after[0].icons[0]
    );
    assert_eq!(
        after[0].icons[1], before[0].icons[0],
        "the remote candidates stay behind it as a fallback"
    );
}
