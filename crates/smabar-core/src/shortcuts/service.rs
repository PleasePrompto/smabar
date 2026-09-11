//! Lazy app search, pin resolution and launching over an injected platform.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::util::lock_unpoisoned;

use crate::config::{ShortcutEntry, ShortcutsConfig, SpecialShortcut};

use super::pins::{EntrySource, entry_source, pin_id, separator_id};
use super::platform::{
    AppSource, IconTarget, InstalledApp, LaunchTarget, PlatformItem, ShortcutPlatform,
};
use super::web;
use super::{ResolvedShortcut, ShortcutError};

struct Inner {
    platform: ShortcutPlatform,
    /// Website and extracted native icons (`~/.smabar/cache/icons/`).
    icons_dir: PathBuf,
    /// Lazily scanned applications; `refresh` clears it.
    apps: Mutex<Option<Arc<Vec<InstalledApp>>>>,
    /// Native/theme icon request -> data URI; misses and failures are cached.
    icon_cache: Mutex<HashMap<IconTarget, Option<String>>>,
}

/// Shared, cheaply clonable handle on the shortcuts subsystem.
#[derive(Clone)]
pub struct ShortcutsService {
    inner: Arc<Inner>,
}

impl ShortcutsService {
    /// The app edge selects and injects the platform implementation.
    pub fn new(platform: ShortcutPlatform, icons_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                platform,
                icons_dir,
                apps: Mutex::new(None),
                icon_cache: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Where website and native icons are cached.
    pub fn icons_dir(&self) -> &Path {
        &self.inner.icons_dir
    }

    /// Drops the app and in-memory icon caches; the next access rescans.
    pub fn refresh(&self) {
        *lock_unpoisoned(&self.inner.apps) = None;
        lock_unpoisoned(&self.inner.icon_cache).clear();
    }

    fn apps(&self) -> Result<Arc<Vec<InstalledApp>>, ShortcutError> {
        let mut apps = lock_unpoisoned(&self.inner.apps);
        if let Some(apps) = apps.as_ref() {
            return Ok(Arc::clone(apps));
        }
        let mut discovered = self.inner.platform.discover_apps()?;
        discovered.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.source.id().cmp(&right.source.id()))
        });
        let discovered = Arc::new(discovered);
        tracing::debug!(count = discovered.len(), "scanned installed applications");
        *apps = Some(Arc::clone(&discovered));
        Ok(discovered)
    }

    /// Case-insensitive substring search over names and comments. Empty lists all.
    pub fn search(&self, query: &str) -> Result<Vec<InstalledApp>, ShortcutError> {
        let needle = query.trim().to_lowercase();
        Ok(self
            .apps()?
            .iter()
            .filter(|app| {
                needle.is_empty()
                    || app.name.to_lowercase().contains(&needle)
                    || app
                        .comment
                        .as_ref()
                        .is_some_and(|comment| comment.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect())
    }

    fn find_app(&self, source: &AppSource) -> Result<Option<InstalledApp>, ShortcutError> {
        Ok(self
            .apps()?
            .iter()
            .find(|app| &app.source == source)
            .cloned())
    }

    /// Resolves pins for display. Broken or structurally invalid persisted pins
    /// remain visible with a fallback label so the user can remove them.
    pub fn resolve_pinned(&self, config: &ShortcutsConfig) -> Vec<ResolvedShortcut> {
        config
            .pinned
            .iter()
            .map(|entry| self.resolve_entry(entry))
            .collect()
    }

    fn resolve_entry(&self, entry: &ShortcutEntry) -> ResolvedShortcut {
        let source = entry_source(entry);
        if matches!(source, Ok(EntrySource::Separator)) {
            return ResolvedShortcut {
                id: entry.id.clone(),
                label: String::new(),
                icons: Vec::new(),
                desktop_id: None,
                separator: true,
            };
        }
        if let Ok(EntrySource::Url(url)) = source {
            let mut icons = Vec::with_capacity(3);
            icons.extend(web::cached_icon(&self.inner.icons_dir, url));
            icons.extend(web::icon_candidates(url));
            return ResolvedShortcut {
                id: entry.id.clone(),
                label: entry.label.clone().unwrap_or_else(|| web::label(url)),
                icons,
                desktop_id: None,
                separator: false,
            };
        }

        let item = source
            .ok()
            .and_then(|source| self.item_for_source(source).ok().flatten());
        let label = entry
            .label
            .clone()
            .or_else(|| item.as_ref().map(|item| item.label.clone()))
            .unwrap_or_else(|| fallback_label(entry));
        let icons = item
            .and_then(|item| item.icon)
            .as_ref()
            .and_then(|icon| self.icon_uri(icon))
            .into_iter()
            .collect();
        ResolvedShortcut {
            id: entry.id.clone(),
            label,
            icons,
            desktop_id: entry.desktop_id.clone(),
            separator: false,
        }
    }

    fn item_for_source(
        &self,
        source: EntrySource<'_>,
    ) -> Result<Option<PlatformItem>, ShortcutError> {
        match source {
            EntrySource::DesktopId(id) => Ok(self
                .find_app(&AppSource::DesktopId(id.to_string()))?
                .map(|app| PlatformItem {
                    label: app.name,
                    icon: app.icon,
                    launch: app.launch,
                })),
            EntrySource::Path(path) => self.inner.platform.inspect_path(path).map(Some),
            EntrySource::Special(special) => Ok(Some(self.inner.platform.special(special))),
            EntrySource::Url(_) | EntrySource::Separator => Ok(None),
        }
    }

    /// Inline icon for one installed-app source from the search result.
    pub fn app_icon_data_uri(&self, desktop_id: &str) -> Option<String> {
        self.app_icon_data_uri_for_source(Some(desktop_id), None)
            .ok()
            .flatten()
    }

    /// Source-aware variant used by cross-platform frontends.
    pub fn app_icon_data_uri_for_source(
        &self,
        desktop_id: Option<&str>,
        path: Option<&Path>,
    ) -> Result<Option<String>, ShortcutError> {
        let source = app_source(desktop_id, path)?;
        let icon = match self.find_app(&source)? {
            Some(app) => app.icon,
            None => match source {
                AppSource::DesktopId(id) => {
                    return Err(ShortcutError::UnknownDesktopId { id });
                }
                AppSource::Path(path) => self.inner.platform.inspect_path(&path)?.icon,
            },
        };
        Ok(icon.as_ref().and_then(|icon| self.icon_uri(icon)))
    }

    fn icon_uri(&self, icon: &IconTarget) -> Option<String> {
        if let Some(cached) = lock_unpoisoned(&self.inner.icon_cache).get(icon) {
            return cached.clone();
        }
        let uri = match self.inner.platform.resolve_icon(icon) {
            Ok(uri) => uri,
            Err(error) => {
                tracing::warn!(
                    icon = ?icon,
                    %error,
                    "shortcut icon extraction failed; using the initial-letter tile"
                );
                None
            }
        };
        lock_unpoisoned(&self.inner.icon_cache).insert(icon.clone(), uri.clone());
        uri
    }

    /// Builds a separator or exactly one validated launch source.
    pub fn validated_entry(
        &self,
        desktop_id: Option<String>,
        path: Option<PathBuf>,
        url: Option<String>,
        label: Option<String>,
        separator: bool,
    ) -> Result<ShortcutEntry, ShortcutError> {
        self.validated_entry_with_special(desktop_id, path, url, None, label, separator)
    }

    /// Source-aware variant that includes curated special items.
    pub fn validated_entry_with_special(
        &self,
        desktop_id: Option<String>,
        path: Option<PathBuf>,
        url: Option<String>,
        special: Option<SpecialShortcut>,
        label: Option<String>,
        separator: bool,
    ) -> Result<ShortcutEntry, ShortcutError> {
        let candidate = ShortcutEntry {
            desktop_id,
            path,
            url,
            special,
            label,
            separator,
            ..ShortcutEntry::default()
        };
        let source = entry_source(&candidate)?;
        let id = match source {
            EntrySource::Separator => {
                return Ok(ShortcutEntry {
                    id: separator_id(),
                    ..candidate
                });
            }
            EntrySource::DesktopId(id) => {
                if self
                    .find_app(&AppSource::DesktopId(id.to_string()))?
                    .is_none()
                {
                    return Err(ShortcutError::UnknownDesktopId { id: id.to_string() });
                }
                pin_id(id)
            }
            EntrySource::Path(path) => {
                self.inner.platform.inspect_path(path)?;
                pin_id(&path.to_string_lossy())
            }
            EntrySource::Url(url) => {
                crate::open::validate_url(url)?;
                pin_id(url)
            }
            EntrySource::Special(special) => pin_id(match special {
                SpecialShortcut::Computer => "special:computer",
                SpecialShortcut::Trash => "special:trash",
            }),
        };
        Ok(ShortcutEntry { id, ..candidate })
    }

    /// Launches one persisted pin after rechecking its exactly-one-source shape.
    pub fn launch_pinned(&self, entry: &ShortcutEntry) -> Result<(), ShortcutError> {
        match entry_source(entry)? {
            EntrySource::Separator => Err(ShortcutError::SeparatorNotLaunchable {
                id: entry.id.clone(),
            }),
            EntrySource::DesktopId(id) => self.launch_desktop_id(id),
            EntrySource::Path(path) => {
                let item = self.inner.platform.inspect_path(path)?;
                self.inner.platform.launch(&item.launch)
            }
            EntrySource::Url(url) => self.open_url(url),
            EntrySource::Special(special) => {
                let item = self.inner.platform.special(special);
                self.inner.platform.launch(&item.launch)
            }
        }
    }

    pub fn launch_desktop_id(&self, desktop_id: &str) -> Result<(), ShortcutError> {
        let app = self
            .find_app(&AppSource::DesktopId(desktop_id.to_string()))?
            .ok_or_else(|| ShortcutError::UnknownDesktopId {
                id: desktop_id.to_string(),
            })?;
        self.inner.platform.launch(&app.launch)
    }

    /// Opens a local file with whatever the desktop associates with it —
    /// a downloaded update package lands in the system installer this way.
    pub fn open_path(&self, path: &Path) -> Result<(), ShortcutError> {
        self.inner.platform.launch(&LaunchTarget::Path {
            name: "smabar update".to_string(),
            path: path.to_path_buf(),
        })
    }

    /// Validates and opens an http(s) URL with the injected native opener.
    pub fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
        crate::open::validate_url(url)?;
        self.inner.platform.open_url(url)
    }
}

fn app_source(desktop_id: Option<&str>, path: Option<&Path>) -> Result<AppSource, ShortcutError> {
    match (desktop_id, path) {
        (Some(id), None) => Ok(AppSource::DesktopId(id.to_string())),
        (None, Some(path)) => Ok(AppSource::Path(path.to_path_buf())),
        _ => Err(ShortcutError::InvalidSource),
    }
}

fn fallback_label(entry: &ShortcutEntry) -> String {
    entry
        .desktop_id
        .as_ref()
        .and_then(|id| id.strip_suffix(".desktop").map(str::to_string))
        .or_else(|| {
            entry.path.as_ref().and_then(|path| {
                path.file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
            })
        })
        .unwrap_or_else(|| entry.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::platform::ShortcutPlatformOps;

    struct BrokenDiscovery;

    impl ShortcutPlatformOps for BrokenDiscovery {
        fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
            Err(ShortcutError::Platform {
                action: "scan test applications",
                source: std::io::Error::other("unavailable"),
            })
        }

        fn inspect_path(&self, _path: &Path) -> Result<PlatformItem, ShortcutError> {
            unreachable!("not used by this test")
        }

        fn resolve_icon(&self, _target: &IconTarget) -> Result<Option<String>, ShortcutError> {
            unreachable!("not used by this test")
        }

        fn launch(&self, _target: &LaunchTarget) -> Result<(), ShortcutError> {
            unreachable!("not used by this test")
        }

        fn special(&self, _special: SpecialShortcut) -> PlatformItem {
            unreachable!("not used by this test")
        }

        fn open_url(&self, _url: &str) -> Result<(), ShortcutError> {
            unreachable!("not used by this test")
        }
    }

    #[test]
    fn app_source_requires_exactly_one_identifier() {
        assert!(matches!(
            app_source(Some("app.desktop"), None),
            Ok(AppSource::DesktopId(_))
        ));
        assert!(matches!(
            app_source(None, Some(Path::new("/app.lnk"))),
            Ok(AppSource::Path(_))
        ));
        assert!(app_source(None, None).is_err());
        assert!(app_source(Some("app.desktop"), Some(Path::new("/app.lnk"))).is_err());
    }

    #[test]
    fn search_reports_platform_discovery_failures() {
        let service = ShortcutsService::new(
            ShortcutPlatform::new(BrokenDiscovery),
            PathBuf::from("unused"),
        );
        assert!(matches!(
            service.search(""),
            Err(ShortcutError::Platform { .. })
        ));
    }

    struct RecordingLauncher(Mutex<Vec<String>>);

    impl ShortcutPlatformOps for RecordingLauncher {
        fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
            Ok(Vec::new())
        }

        fn inspect_path(&self, _path: &Path) -> Result<PlatformItem, ShortcutError> {
            unreachable!("not used by this test")
        }

        fn resolve_icon(&self, _target: &IconTarget) -> Result<Option<String>, ShortcutError> {
            unreachable!("not used by this test")
        }

        fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
            lock_unpoisoned(&self.0).push(format!("{target:?}"));
            Ok(())
        }

        fn special(&self, _special: SpecialShortcut) -> PlatformItem {
            unreachable!("not used by this test")
        }

        fn open_url(&self, _url: &str) -> Result<(), ShortcutError> {
            unreachable!("not used by this test")
        }
    }

    #[test]
    fn open_path_launches_the_file_through_the_platform_opener() {
        let launcher = Arc::new(RecordingLauncher(Mutex::new(Vec::new())));
        let service = ShortcutsService::new(
            ShortcutPlatform::new(SharedLauncher(Arc::clone(&launcher))),
            PathBuf::from("unused"),
        );
        service
            .open_path(Path::new("/tmp/smabar_0.2.0_amd64.deb"))
            .expect("opening a path is delegated, never refused here");
        let launched = lock_unpoisoned(&launcher.0);
        assert_eq!(launched.len(), 1);
        assert!(launched[0].contains("smabar update"));
        assert!(launched[0].contains("smabar_0.2.0_amd64.deb"));
    }

    /// `ShortcutPlatform::new` takes ownership, so the test keeps a handle
    /// through this forwarding wrapper.
    struct SharedLauncher(Arc<RecordingLauncher>);

    impl ShortcutPlatformOps for SharedLauncher {
        fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
            self.0.discover_apps()
        }

        fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError> {
            self.0.inspect_path(path)
        }

        fn resolve_icon(&self, target: &IconTarget) -> Result<Option<String>, ShortcutError> {
            self.0.resolve_icon(target)
        }

        fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
            self.0.launch(target)
        }

        fn special(&self, special: SpecialShortcut) -> PlatformItem {
            self.0.special(special)
        }

        fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
            self.0.open_url(url)
        }
    }
}
