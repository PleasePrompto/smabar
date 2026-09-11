//! XDG `.desktop` file discovery and a minimal `[Desktop Entry]` parser.
//!
//! Deliberately pragmatic (no new dependency): only the keys the bar needs,
//! only exact `Name[<lang>]` locale matches, and no terminal applications in
//! v1 (`Terminal=true` entries are skipped — launching them would require a
//! terminal-emulator picker).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One launchable application from a `.desktop` file.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    /// XDG desktop-file id: path relative to an applications directory with
    /// `/` replaced by `-`, e.g. `org.gnome.Calculator.desktop`.
    pub desktop_id: String,
    pub name: String,
    pub comment: Option<String>,
    /// Raw `Exec` line (field codes still included).
    pub exec: String,
    /// Icon name or absolute path, as written in the file.
    pub icon: Option<String>,
    /// Working directory from the `Path` key.
    pub working_dir: Option<PathBuf>,
}

/// The keys of a parsed `[Desktop Entry]` section, before filtering.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DesktopEntry {
    pub name: String,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub working_dir: Option<PathBuf>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
}

/// The standard XDG applications directories, from environment values the
/// caller read: `$XDG_DATA_HOME` (fallback `~/.local/share`) followed by
/// `$XDG_DATA_DIRS` (fallback `/usr/local/share:/usr/share`), each with
/// `/applications` appended. Earlier directories take precedence.
pub fn default_search_dirs(
    xdg_data_home: Option<&str>,
    xdg_data_dirs: Option<&str>,
    home: &Path,
) -> Vec<PathBuf> {
    let data_home = match xdg_data_home {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => home.join(".local/share"),
    };
    let data_dirs = match xdg_data_dirs {
        Some(dirs) if !dirs.trim().is_empty() => dirs.split(':').map(PathBuf::from).collect(),
        _ => vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ],
    };
    let mut result = Vec::new();
    for dir in std::iter::once(data_home).chain(data_dirs) {
        let apps = dir.join("applications");
        if !result.contains(&apps) {
            result.push(apps);
        }
    }
    result
}

/// Scans the given applications directories recursively for launchable apps.
/// Earlier directories win on duplicate desktop ids. Returns the apps sorted
/// by name (case-insensitive).
pub(crate) fn discover_apps(search_dirs: &[PathBuf], lang: Option<&str>) -> Vec<DesktopApp> {
    let mut by_id: BTreeMap<String, DesktopApp> = BTreeMap::new();
    for dir in search_dirs {
        let mut files = Vec::new();
        collect_desktop_files(dir, dir, &mut files);
        for (path, relative) in files {
            let desktop_id = relative.replace('/', "-");
            if by_id.contains_key(&desktop_id) {
                continue; // earlier search dir wins
            }
            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(error) => {
                    tracing::debug!(path = %path.display(), %error, "skipping unreadable .desktop file");
                    continue;
                }
            };
            let Some(entry) = parse_desktop_entry(&content, lang) else {
                continue;
            };
            if entry.no_display || entry.hidden || entry.terminal {
                continue;
            }
            by_id.insert(
                desktop_id.clone(),
                DesktopApp {
                    desktop_id,
                    name: entry.name,
                    comment: entry.comment,
                    exec: entry.exec,
                    icon: entry.icon,
                    working_dir: entry.working_dir,
                },
            );
        }
    }
    let mut apps: Vec<DesktopApp> = by_id.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// Collects `.desktop` files under `dir` as `(path, relative_path)` pairs.
fn collect_desktop_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(dir = %dir.display(), %error, "cannot read applications directory");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_files(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "desktop")
            && let Ok(relative) = path.strip_prefix(root)
        {
            let relative = relative.to_string_lossy().replace('\\', "/");
            out.push((path, relative));
        }
    }
}

/// Parses the `[Desktop Entry]` section. Returns `None` when the file is not
/// a `Type=Application` entry with both `Name` and `Exec`.
pub(crate) fn parse_desktop_entry(content: &str, lang: Option<&str>) -> Option<DesktopEntry> {
    let localized_name_key = lang.map(|lang| format!("Name[{lang}]"));
    let mut in_entry_section = false;
    let mut is_application = false;
    let mut name = None;
    let mut localized_name = None;
    let mut comment = None;
    let mut exec = None;
    let mut icon = None;
    let mut working_dir = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut terminal = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Type" => is_application = value == "Application",
            "Name" => name = Some(value.to_string()),
            "Comment" => comment = Some(value.to_string()).filter(|c| !c.is_empty()),
            "Exec" => exec = Some(value.to_string()).filter(|e| !e.is_empty()),
            "Icon" => icon = Some(value.to_string()).filter(|i| !i.is_empty()),
            "Path" => {
                working_dir = Some(PathBuf::from(value)).filter(|p| !p.as_os_str().is_empty())
            }
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            "Terminal" => terminal = value == "true",
            other if localized_name_key.as_deref() == Some(other) => {
                localized_name = Some(value.to_string());
            }
            _ => {}
        }
    }

    if !is_application {
        return None;
    }
    Some(DesktopEntry {
        name: localized_name.or(name).filter(|n| !n.is_empty())?,
        comment,
        exec: exec?,
        icon,
        working_dir,
        no_display,
        hidden,
        terminal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, relative: &str, content: &str) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
        fs::write(path, content).expect("write fixture");
    }

    const CALC: &str = "[Desktop Entry]\nType=Application\nName=Calculator\n\
                        Name[de]=Rechner\nComment=Do math\nExec=calc %u\nIcon=calc\n";

    #[test]
    fn default_search_dirs_prefer_env_values_and_deduplicate() {
        let home = Path::new("/home/test");
        let dirs = default_search_dirs(None, None, home);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/home/test/.local/share/applications"),
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        );

        let dirs = default_search_dirs(Some("/data"), Some("/data:/opt/share"), home);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/data/applications"),
                PathBuf::from("/opt/share/applications"),
            ]
        );
    }

    #[test]
    fn parser_reads_the_entry_keys_and_prefers_the_localized_name() {
        let entry = parse_desktop_entry(CALC, Some("de")).expect("parse");
        assert_eq!(entry.name, "Rechner");
        assert_eq!(entry.comment.as_deref(), Some("Do math"));
        assert_eq!(entry.exec, "calc %u");
        assert_eq!(entry.icon.as_deref(), Some("calc"));
        assert!(!entry.terminal);

        let entry = parse_desktop_entry(CALC, Some("fr")).expect("parse");
        assert_eq!(entry.name, "Calculator");
        let entry = parse_desktop_entry(CALC, None).expect("parse");
        assert_eq!(entry.name, "Calculator");
    }

    #[test]
    fn parser_rejects_non_applications_and_ignores_other_sections() {
        assert!(
            parse_desktop_entry("[Desktop Entry]\nType=Link\nName=A\nExec=a\n", None).is_none()
        );
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\nName=A\n", None).is_none());
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\nExec=a\n", None).is_none());

        let with_action = "[Desktop Entry]\nType=Application\nName=A\nExec=a\n\
                           [Desktop Action new]\nName=Shadowed\nExec=other\n";
        let entry = parse_desktop_entry(with_action, None).expect("parse");
        assert_eq!(entry.name, "A");
        assert_eq!(entry.exec, "a");
    }

    #[test]
    fn discovery_filters_hidden_terminal_and_nodisplay_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let apps = dir.path();
        write(apps, "calc.desktop", CALC);
        write(
            apps,
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=H\nExec=h\nHidden=true\n",
        );
        write(
            apps,
            "nodisplay.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=n\nNoDisplay=true\n",
        );
        write(
            apps,
            "term.desktop",
            "[Desktop Entry]\nType=Application\nName=T\nExec=t\nTerminal=true\n",
        );
        write(apps, "link.desktop", "[Desktop Entry]\nType=Link\nName=L\n");
        write(apps, "notes.txt", "not a desktop file");

        let found = discover_apps(&[apps.to_path_buf()], None);
        let ids: Vec<&str> = found.iter().map(|a| a.desktop_id.as_str()).collect();
        assert_eq!(ids, vec!["calc.desktop"]);
    }

    #[test]
    fn discovery_derives_nested_ids_and_earlier_dirs_win() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        write(
            first.path(),
            "sub/tool.desktop",
            "[Desktop Entry]\nType=Application\nName=First Tool\nExec=first\n",
        );
        write(
            second.path(),
            "sub/tool.desktop",
            "[Desktop Entry]\nType=Application\nName=Second Tool\nExec=second\n",
        );
        write(
            second.path(),
            "other.desktop",
            "[Desktop Entry]\nType=Application\nName=Other\nExec=other\n",
        );

        let found = discover_apps(
            &[first.path().to_path_buf(), second.path().to_path_buf()],
            None,
        );
        let pairs: Vec<(&str, &str)> = found
            .iter()
            .map(|a| (a.desktop_id.as_str(), a.exec.as_str()))
            .collect();
        // Sorted by name; the nested id uses `-`, and the first dir wins.
        assert_eq!(
            pairs,
            vec![("sub-tool.desktop", "first"), ("other.desktop", "other")]
        );
    }
}
