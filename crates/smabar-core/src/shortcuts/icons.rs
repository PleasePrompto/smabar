//! Pragmatic icon resolution — deliberately WITHOUT `index.theme`
//! inheritance parsing: the active GTK icon theme (plus its stripped suffix
//! variants) first, then hicolor, then other themes alphabetically, a fixed
//! set of size directories per context, and a flat pixmaps fallback. Misses
//! are logged at debug level and render as an initial-letter tile in the
//! shell.

use std::fs;
use std::path::{Path, PathBuf};

/// Icon size directories to probe, in preference order, each per context.
const SIZE_DIRS: [&str; 5] = ["64x64", "48x48", "128x128", "256x256", "scalable"];
/// Icon file extensions to probe, in preference order.
const EXTENSIONS: [&str; 2] = ["svg", "png"];
/// Theme context directories to probe, in preference order: applications
/// first, then `places` (folder icons like `folder`), `devices` (the
/// `computer` pin — a Devices icon by the naming spec) and `mimetypes`.
/// Probing only `apps` was why folder pins fell back to the initial tile;
/// skipping `devices` did the same to Computer.
const CONTEXTS: [&str; 4] = ["apps", "places", "devices", "mimetypes"];

/// Where to look for icons. Built once at the app edge (from environment
/// values) and injected, so resolution stays testable.
#[derive(Debug, Clone, Default)]
pub struct IconDirs {
    /// Roots containing theme directories (`<root>/<theme>/<size>/<context>/`
    /// with the contexts apps/places/devices/mimetypes), e.g. `~/.icons`,
    /// `$XDG_DATA_HOME/icons`, `/usr/share/icons`.
    pub theme_roots: Vec<PathBuf>,
    /// Flat directories probed as `<dir>/<name>.<ext>`, e.g.
    /// `/usr/share/pixmaps`.
    pub flat_dirs: Vec<PathBuf>,
    /// The user's active icon theme (GTK `gtk-icon-theme-name`), probed
    /// before every other theme. `None` keeps the plain hicolor-first order.
    pub preferred_theme: Option<String>,
}

/// Extracts the active icon theme name from the CONTENT of a GTK
/// `settings.ini` (key `gtk-icon-theme-name`; ini sections are irrelevant
/// for this single key). The file itself is read at the app edge and this
/// stays a pure, testable function.
pub fn icon_theme_from_settings_ini(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "gtk-icon-theme-name" {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Theme names to probe first, in order: the active theme, then its
/// progressively stripped suffix variants (`Mint-Y-Sand` → `Mint-Y` →
/// `Mint`) — a pragmatic stand-in for `index.theme` inheritance, which
/// deliberately stays unparsed — then `hicolor`.
fn theme_priority(preferred: Option<&str>) -> Vec<String> {
    let mut priority = Vec::new();
    if let Some(preferred) = preferred {
        let mut name = preferred.trim();
        while !name.is_empty() {
            priority.push(name.to_string());
            let Some(pos) = name.rfind('-') else {
                break;
            };
            name = &name[..pos];
        }
    }
    if !priority.iter().any(|name| name == "hicolor") {
        priority.push("hicolor".to_string());
    }
    priority
}

/// The standard icon locations: `~/.icons`, `$XDG_DATA_HOME/icons`
/// (fallback `~/.local/share/icons`), every `$XDG_DATA_DIRS` entry's
/// `icons/` (fallback `/usr/local/share:/usr/share` — this is what makes
/// Flatpak-exported icons resolve, their export dir is on XDG_DATA_DIRS),
/// with `/usr/share/pixmaps` as the flat fallback.
pub fn default_icon_dirs(
    xdg_data_home: Option<&str>,
    xdg_data_dirs: Option<&str>,
    home: &Path,
) -> IconDirs {
    let data_home = match xdg_data_home {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => home.join(".local/share"),
    };
    let data_dirs: Vec<PathBuf> = match xdg_data_dirs {
        Some(dirs) if !dirs.trim().is_empty() => dirs.split(':').map(PathBuf::from).collect(),
        _ => vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ],
    };
    let mut theme_roots = vec![home.join(".icons"), data_home.join("icons")];
    for dir in data_dirs {
        let icons = dir.join("icons");
        if !theme_roots.contains(&icons) {
            theme_roots.push(icons);
        }
    }
    IconDirs {
        theme_roots,
        flat_dirs: vec![PathBuf::from("/usr/share/pixmaps")],
        preferred_theme: None,
    }
}

/// Resolves a `.desktop` `Icon` value (absolute path or theme icon name) to
/// an existing file. Returns `None` on a miss (logged at debug level).
pub(crate) fn resolve_icon(name_or_path: &str, dirs: &IconDirs) -> Option<PathBuf> {
    let raw = Path::new(name_or_path);
    if raw.is_absolute() {
        if raw.is_file() {
            return Some(raw.to_path_buf());
        }
        tracing::debug!(icon = name_or_path, "absolute icon path does not exist");
        return None;
    }
    // Some .desktop files wrongly include an extension in the icon name.
    let name = name_or_path
        .strip_suffix(".png")
        .or_else(|| name_or_path.strip_suffix(".svg"))
        .or_else(|| name_or_path.strip_suffix(".xpm"))
        .unwrap_or(name_or_path);

    let priority = theme_priority(dirs.preferred_theme.as_deref());
    for root in &dirs.theme_roots {
        for theme in themes_in(root, &priority) {
            for context in CONTEXTS {
                for size in SIZE_DIRS {
                    for ext in EXTENSIONS {
                        let candidate =
                            theme.join(size).join(context).join(format!("{name}.{ext}"));
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }
    for dir in &dirs.flat_dirs {
        for ext in EXTENSIONS {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    tracing::debug!(icon = name_or_path, "icon not found in any icon directory");
    None
}

/// Theme directories under `root`: the [`theme_priority`] names first (in
/// that order), then the rest alphabetically. A missing root yields nothing.
fn themes_in(root: &Path, priority: &[String]) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut themes: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    themes.sort();
    // Stable sort: prioritized themes move to the front in priority order,
    // everything else keeps the alphabetical order.
    themes.sort_by_key(|path| {
        let name = path.file_name().map(|name| name.to_string_lossy());
        priority
            .iter()
            .position(|candidate| name.as_deref() == Some(candidate))
            .unwrap_or(usize::MAX)
    });
    themes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
        fs::write(path, b"icon").expect("write icon");
    }

    #[test]
    fn default_icon_dirs_include_data_dirs_roots_and_deduplicate() {
        let home = Path::new("/home/test");
        let dirs = default_icon_dirs(None, None, home);
        assert_eq!(
            dirs.theme_roots,
            vec![
                PathBuf::from("/home/test/.icons"),
                PathBuf::from("/home/test/.local/share/icons"),
                PathBuf::from("/usr/local/share/icons"),
                PathBuf::from("/usr/share/icons"),
            ]
        );

        // Flatpak-style XDG_DATA_DIRS entries become icon roots; duplicates
        // of the data-home root are skipped.
        let dirs = default_icon_dirs(
            Some("/data"),
            Some("/var/lib/flatpak/exports/share:/data:/usr/share"),
            home,
        );
        assert_eq!(
            dirs.theme_roots,
            vec![
                PathBuf::from("/home/test/.icons"),
                PathBuf::from("/data/icons"),
                PathBuf::from("/var/lib/flatpak/exports/share/icons"),
                PathBuf::from("/usr/share/icons"),
            ]
        );
    }

    #[test]
    fn absolute_paths_resolve_directly_or_miss() {
        let dir = tempfile::tempdir().expect("tempdir");
        let icon = dir.path().join("logo.png");
        touch(&icon);
        let dirs = IconDirs::default();
        assert_eq!(
            resolve_icon(&icon.to_string_lossy(), &dirs),
            Some(icon.clone())
        );
        assert_eq!(
            resolve_icon(&dir.path().join("missing.png").to_string_lossy(), &dirs),
            None
        );
    }

    #[test]
    fn hicolor_wins_over_other_themes_and_svg_over_png() {
        let root = tempfile::tempdir().expect("tempdir");
        let hicolor = root.path().join("hicolor/48x48/apps/calc.png");
        let adwaita = root.path().join("Adwaita/48x48/apps/calc.png");
        touch(&hicolor);
        touch(&adwaita);
        let dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            ..IconDirs::default()
        };
        assert_eq!(resolve_icon("calc", &dirs), Some(hicolor.clone()));

        // Within a theme, the earlier size dir and svg extension win.
        let svg = root.path().join("hicolor/64x64/apps/calc.svg");
        touch(&svg);
        assert_eq!(resolve_icon("calc", &dirs), Some(svg));
    }

    #[test]
    fn places_icons_resolve_and_apps_wins_over_places() {
        let root = tempfile::tempdir().expect("tempdir");
        let places = root.path().join("hicolor/64x64/places/folder.svg");
        touch(&places);
        let dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            ..IconDirs::default()
        };
        // Folder icons live in the `places` context (the folder-pin fix).
        assert_eq!(resolve_icon("folder", &dirs), Some(places));

        // When a name exists in both contexts, `apps` wins.
        let apps = root.path().join("hicolor/48x48/apps/folder.png");
        touch(&apps);
        assert_eq!(resolve_icon("folder", &dirs), Some(apps));
    }

    #[test]
    fn device_icons_resolve_so_the_computer_pin_gets_its_system_icon() {
        let root = tempfile::tempdir().expect("tempdir");
        let computer = root.path().join("hicolor/scalable/devices/computer.svg");
        touch(&computer);
        let dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            flat_dirs: vec![],
            preferred_theme: None,
        };
        assert_eq!(resolve_icon("computer", &dirs), Some(computer));
    }

    #[test]
    fn active_theme_beats_alphabetically_earlier_themes() {
        // Live-reported bug: with the active theme "Mint-Y-Sand" and no
        // hicolor hit, the byte-sorted order made "HighContrast" win.
        let root = tempfile::tempdir().expect("tempdir");
        let contrast = root.path().join("HighContrast/48x48/apps/term.png");
        let mint = root.path().join("Mint-Y-Sand/48x48/apps/term.png");
        touch(&contrast);
        touch(&mint);
        let mut dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            ..IconDirs::default()
        };
        assert_eq!(
            resolve_icon("term", &dirs),
            Some(contrast),
            "without a preferred theme the alphabetical order stands"
        );
        dirs.preferred_theme = Some("Mint-Y-Sand".to_string());
        assert_eq!(resolve_icon("term", &dirs), Some(mint));
    }

    #[test]
    fn stripped_theme_variants_fill_in_and_hicolor_stays_before_the_rest() {
        let root = tempfile::tempdir().expect("tempdir");
        let base = root.path().join("Mint-Y/48x48/apps/term.png");
        let contrast = root.path().join("HighContrast/48x48/apps/other.png");
        let hicolor = root.path().join("hicolor/48x48/apps/other.png");
        touch(&base);
        touch(&contrast);
        touch(&hicolor);
        let dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            preferred_theme: Some("Mint-Y-Sand".to_string()),
            ..IconDirs::default()
        };
        // "Mint-Y-Sand" has no hit, its stripped variant "Mint-Y" does.
        assert_eq!(resolve_icon("term", &dirs), Some(base));
        // hicolor still ranks before alphabetically earlier themes.
        assert_eq!(resolve_icon("other", &dirs), Some(hicolor));
    }

    #[test]
    fn theme_priority_strips_suffix_segments_and_ends_in_hicolor() {
        assert_eq!(
            theme_priority(Some("Mint-Y-Sand")),
            vec!["Mint-Y-Sand", "Mint-Y", "Mint", "hicolor"]
        );
        assert_eq!(theme_priority(Some("Adwaita")), vec!["Adwaita", "hicolor"]);
        assert_eq!(theme_priority(Some("hicolor")), vec!["hicolor"]);
        assert_eq!(theme_priority(None), vec!["hicolor"]);
    }

    #[test]
    fn settings_ini_parser_finds_the_icon_theme_key() {
        let ini = "[Settings]\n# a comment\ngtk-theme-name=Mint-Y\n\
                   gtk-icon-theme-name = Mint-Y-Sand\n; another comment\n";
        assert_eq!(
            icon_theme_from_settings_ini(ini),
            Some("Mint-Y-Sand".to_string())
        );
        // Missing key, commented-out key, empty value, empty file → None.
        assert_eq!(
            icon_theme_from_settings_ini("gtk-theme-name=Mint-Y\n"),
            None
        );
        assert_eq!(
            icon_theme_from_settings_ini("# gtk-icon-theme-name=Nope\n"),
            None
        );
        assert_eq!(icon_theme_from_settings_ini("gtk-icon-theme-name=\n"), None);
        assert_eq!(icon_theme_from_settings_ini(""), None);
    }

    #[test]
    fn falls_back_to_pixmaps_and_strips_bogus_extensions() {
        let root = tempfile::tempdir().expect("tempdir");
        let pixmaps = tempfile::tempdir().expect("tempdir");
        let icon = pixmaps.path().join("flat.png");
        touch(&icon);
        let dirs = IconDirs {
            theme_roots: vec![root.path().to_path_buf()],
            flat_dirs: vec![pixmaps.path().to_path_buf()],
            ..IconDirs::default()
        };
        assert_eq!(resolve_icon("flat", &dirs), Some(icon.clone()));
        // Icon names that wrongly include the extension still resolve.
        assert_eq!(resolve_icon("flat.png", &dirs), Some(icon));
        assert_eq!(resolve_icon("nope", &dirs), None);
    }
}
