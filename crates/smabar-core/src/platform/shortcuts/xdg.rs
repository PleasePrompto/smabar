//! XDG desktop-entry, icon-theme and fixed-opener implementation.

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use gio::prelude::*;

use crate::config::SpecialShortcut;
use crate::shortcuts::desktop::{self, DesktopEntry};
use crate::shortcuts::iconfile::data_uri_for_file;
use crate::shortcuts::icons::{self, IconDirs};
use crate::shortcuts::launch;
use crate::shortcuts::platform::{
    AppSource, IconTarget, InstalledApp, LaunchTarget, PlatformItem, ShortcutPlatformOps,
};
use crate::shortcuts::{ShortcutError, ShortcutPlatform};

const OPENER: &str = "xdg-open";
const FILE_ICON: &str = "text-x-generic";
const FOLDER_ICON: &str = "folder";

struct XdgShortcuts {
    search_dirs: Vec<PathBuf>,
    icon_dirs: IconDirs,
    lang: Option<String>,
    home: PathBuf,
}

/// Creates the existing XDG behavior as an injected platform handle.
pub fn new(
    search_dirs: Vec<PathBuf>,
    icon_dirs: IconDirs,
    lang: Option<String>,
    home: PathBuf,
) -> ShortcutPlatform {
    ShortcutPlatform::new(XdgShortcuts {
        search_dirs,
        icon_dirs,
        lang,
        home,
    })
}

impl ShortcutPlatformOps for XdgShortcuts {
    fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
        Ok(
            desktop::discover_apps(&self.search_dirs, self.lang.as_deref())
                .into_iter()
                .map(|app| InstalledApp {
                    source: AppSource::DesktopId(app.desktop_id),
                    name: app.name.clone(),
                    comment: app.comment,
                    icon: app.icon.map(icon_target),
                    launch: LaunchTarget::Desktop {
                        name: app.name,
                        exec: app.exec,
                        working_dir: app.working_dir,
                        terminal: false,
                    },
                })
                .collect(),
        )
    }

    fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError> {
        if !path.is_absolute() {
            return Err(ShortcutError::InvalidDesktopPath {
                path: path.to_path_buf(),
            });
        }
        if path.is_dir() {
            return Ok(PlatformItem {
                label: path_label(path),
                icon: Some(path_icon_target(path, FOLDER_ICON)),
                launch: LaunchTarget::Path {
                    name: path_label(path),
                    path: path.to_path_buf(),
                },
            });
        }
        if !path.is_file() {
            return Err(ShortcutError::InvalidDesktopPath {
                path: path.to_path_buf(),
            });
        }
        if path
            .extension()
            .is_none_or(|extension| extension != "desktop")
        {
            let label = path_label(path);
            return Ok(PlatformItem {
                label: label.clone(),
                icon: Some(path_icon_target(path, FILE_ICON)),
                launch: LaunchTarget::Path {
                    name: label,
                    path: path.to_path_buf(),
                },
            });
        }
        let content =
            fs::read_to_string(path).map_err(|source| ShortcutError::ReadDesktopFile {
                path: path.to_path_buf(),
                source,
            })?;
        let entry =
            desktop::parse_desktop_entry(&content, self.lang.as_deref()).ok_or_else(|| {
                ShortcutError::NotAnApplication {
                    path: path.to_path_buf(),
                }
            })?;
        Ok(desktop_item(entry))
    }

    fn resolve_icon(&self, target: &IconTarget) -> Result<Option<String>, ShortcutError> {
        let uri = match target {
            IconTarget::Theme(names) => names.iter().find_map(|name| {
                icons::resolve_icon(name, &self.icon_dirs)
                    .as_deref()
                    .and_then(data_uri_for_file)
            }),
            IconTarget::Path(path) => data_uri_for_file(path),
            IconTarget::Special(special) => {
                let (_, _, icon) = special_parts(*special);
                icons::resolve_icon(icon, &self.icon_dirs)
                    .as_deref()
                    .and_then(data_uri_for_file)
            }
        };
        Ok(uri)
    }

    fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
        match target {
            LaunchTarget::Desktop {
                name,
                exec,
                working_dir,
                terminal,
            } => self.launch_desktop(name, exec, working_dir.as_deref(), *terminal),
            LaunchTarget::Path { name, path } => self.spawn(name, &opener_argv(path.as_os_str())),
            LaunchTarget::Special(special) => {
                let (label, uri, _) = special_parts(*special);
                self.spawn(label, &opener_argv(uri.as_ref()))
            }
        }
    }

    fn special(&self, special: SpecialShortcut) -> PlatformItem {
        let (label, _, _) = special_parts(special);
        PlatformItem {
            label: label.to_string(),
            icon: Some(IconTarget::Special(special)),
            launch: LaunchTarget::Special(special),
        }
    }

    fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
        self.spawn("system browser", &opener_argv(url.as_ref()))
    }
}

impl XdgShortcuts {
    fn launch_desktop(
        &self,
        name: &str,
        exec: &str,
        working_dir: Option<&Path>,
        terminal: bool,
    ) -> Result<(), ShortcutError> {
        if terminal {
            return Err(ShortcutError::TerminalApp {
                name: name.to_string(),
            });
        }
        let argv = launch::exec_to_argv(exec);
        if argv.is_empty() {
            return Err(ShortcutError::EmptyExec {
                name: name.to_string(),
            });
        }
        let cwd = working_dir.filter(|dir| dir.is_dir()).unwrap_or(&self.home);
        let pid = launch::spawn_detached(&argv, cwd).map_err(|source| ShortcutError::Spawn {
            name: name.to_string(),
            source,
        })?;
        tracing::info!(app = name, pid, "launched shortcut (detached)");
        Ok(())
    }

    fn spawn(&self, name: &str, argv: &[String]) -> Result<(), ShortcutError> {
        let pid =
            launch::spawn_detached(argv, &self.home).map_err(|source| ShortcutError::Spawn {
                name: name.to_string(),
                source,
            })?;
        tracing::info!(item = name, pid, "opened shortcut target (detached)");
        Ok(())
    }
}

fn desktop_item(entry: DesktopEntry) -> PlatformItem {
    PlatformItem {
        label: entry.name.clone(),
        icon: entry.icon.map(icon_target),
        launch: LaunchTarget::Desktop {
            name: entry.name,
            exec: entry.exec,
            working_dir: entry.working_dir,
            terminal: entry.terminal,
        },
    }
}

fn path_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn icon_target(icon: String) -> IconTarget {
    let path = PathBuf::from(&icon);
    if path.is_absolute() {
        IconTarget::Path(path)
    } else {
        IconTarget::Theme(vec![icon])
    }
}

fn path_icon_target(path: &Path, fallback: &str) -> IconTarget {
    #[cfg(target_os = "linux")]
    {
        let file = gio::File::for_path(path);
        match file.query_info(
            "standard::icon",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        ) {
            Ok(info) => {
                if let Some(target) = info.icon().and_then(gio_icon_target) {
                    return match target {
                        IconTarget::Theme(mut names) => {
                            if !names.iter().any(|name| name == fallback) {
                                names.push(fallback.to_string());
                            }
                            IconTarget::Theme(names)
                        }
                        target => target,
                    };
                }
                tracing::debug!(
                    path = %path.display(),
                    fallback,
                    "system returned no supported shortcut icon; using the generic theme icon"
                );
            }
            Err(error) => tracing::debug!(
                path = %path.display(),
                %error,
                fallback,
                "system shortcut icon lookup failed; using the generic theme icon"
            ),
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = path;
    IconTarget::Theme(vec![fallback.to_string()])
}

#[cfg(target_os = "linux")]
fn gio_icon_target(icon: gio::Icon) -> Option<IconTarget> {
    if let Ok(themed) = icon.clone().downcast::<gio::ThemedIcon>() {
        let names = themed
            .names()
            .into_iter()
            .map(|name| name.to_string())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();
        return (!names.is_empty()).then_some(IconTarget::Theme(names));
    }
    icon.downcast::<gio::FileIcon>()
        .ok()
        .and_then(|icon| icon.file().path())
        .map(IconTarget::Path)
}

fn special_parts(special: SpecialShortcut) -> (&'static str, &'static str, &'static str) {
    match special {
        SpecialShortcut::Computer => ("Computer", "computer:///", "computer"),
        SpecialShortcut::Trash => ("Trash", "trash:///", "user-trash"),
    }
}

fn opener_argv(target: &std::ffi::OsStr) -> [String; 2] {
    [OPENER.to_string(), target.to_string_lossy().into_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_items_use_fixed_uris_and_theme_icons() {
        assert_eq!(
            special_parts(SpecialShortcut::Computer),
            ("Computer", "computer:///", "computer")
        );
        assert_eq!(
            special_parts(SpecialShortcut::Trash),
            ("Trash", "trash:///", "user-trash")
        );
    }

    #[test]
    fn opener_argv_keeps_the_target_as_one_argument() {
        assert_eq!(
            opener_argv(Path::new("/tmp/a folder").as_os_str()),
            ["xdg-open".to_string(), "/tmp/a folder".to_string()]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn path_icons_use_the_systems_ordered_mime_fallbacks() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, contents, expected) in [
            ("page.html", b"<!doctype html>".as_slice(), "text-html"),
            (
                "report.pdf",
                b"%PDF-1.7\n%%EOF\n".as_slice(),
                "application-pdf",
            ),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, contents).expect("write document fixture");
            let IconTarget::Theme(names) = path_icon_target(&path, FILE_ICON) else {
                panic!("documents should use system theme icons");
            };
            assert_eq!(names.first().map(String::as_str), Some(expected));
            assert!(names.iter().any(|name| name == FILE_ICON));
        }
    }
}
