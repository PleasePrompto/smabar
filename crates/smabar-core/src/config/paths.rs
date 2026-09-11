//! Central path layout for everything under the smabar base directory.

use std::path::{Path, PathBuf};

/// Path layout under the smabar base directory (`~/.smabar` by default).
///
/// All modules take paths through this struct instead of calling `dirs::`
/// directly, so tests can point everything at a temp directory.
#[derive(Debug, Clone)]
pub struct SmabarPaths {
    base: PathBuf,
}

impl SmabarPaths {
    /// Use `base_dir` as the smabar base directory.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base: base_dir }
    }

    /// The default base directory `~/.smabar`, or `None` when no home
    /// directory can be determined.
    pub fn default_base() -> Option<Self> {
        dirs::home_dir().map(|home| Self::new(home.join(".smabar")))
    }

    /// The base directory itself.
    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    /// The main config file (`config.json`).
    pub fn config_file(&self) -> PathBuf {
        self.base.join("config.json")
    }

    /// The accepted terms of use (`legal.json`), beside `config.json`: outside
    /// `data/` (orphan sweep), `plugins/` (watched) and `cache/` (disposable).
    pub fn legal_file(&self) -> PathBuf {
        self.base.join("legal.json")
    }

    /// OS-owned Linux login entry. The app supplies the resolved XDG directory.
    pub fn desktop_autostart_file(config_dir: &Path) -> PathBuf {
        config_dir.join("autostart").join("smabar.desktop")
    }

    /// Directory for drop-in locale files (`locales/`).
    pub fn locales_dir(&self) -> PathBuf {
        self.base.join("locales")
    }

    /// Directory for JSONL log files (`logs/`).
    pub fn logs_dir(&self) -> PathBuf {
        self.base.join("logs")
    }

    /// Directory for installed plugins (`plugins/`).
    pub fn plugins_dir(&self) -> PathBuf {
        self.base.join("plugins")
    }

    /// Directory for drop-in theme files (`themes/`).
    pub fn themes_dir(&self) -> PathBuf {
        self.base.join("themes")
    }

    /// Directory for tools provisioned by bundled runtimes (`tools/`).
    pub fn tools_dir(&self) -> PathBuf {
        self.base.join("tools")
    }

    /// Root of the writable per-plugin data directories (`data/`).
    ///
    /// Deliberately OUTSIDE `plugins/`: the supervisor watches the plugin
    /// folder recursively and restarts on any write, so a plugin persisting
    /// state next to its own code restarts itself in a loop.
    pub fn data_dir(&self) -> PathBuf {
        self.base.join("data")
    }

    /// The writable directory a single plugin owns (`data/<plugin-id>/`).
    /// Removed together with the plugin.
    pub fn plugin_data_dir(&self, plugin_id: &str) -> PathBuf {
        self.data_dir().join(plugin_id)
    }

    /// Cache for downloaded website icons (`cache/icons/`). Disposable:
    /// deleting it only costs one refetch.
    pub fn icons_dir(&self) -> PathBuf {
        self.base.join("cache").join("icons")
    }

    /// Cache for Google Fonts downloaded for smabar (`cache/fonts/`).
    /// Fonts are registered only inside the webview, never installed into
    /// the operating system's font directories.
    pub fn fonts_dir(&self) -> PathBuf {
        self.base.join("cache").join("fonts")
    }

    /// Locally cached Google Fonts (`cache/fonts/google/`).
    pub fn google_fonts_dir(&self) -> PathBuf {
        self.fonts_dir().join("google")
    }

    /// Downloaded application update packages (`cache/updates/`), one at a
    /// time; the system installer reads them from here. Disposable.
    pub fn updates_dir(&self) -> PathBuf {
        self.base.join("cache").join("updates")
    }

    /// Bookkeeping for seeded bundled plugins (`cache/seed.json`).
    ///
    /// Lives under `cache/`, not `data/`, so the orphan sweep over
    /// `data/<plugin-id>/` cannot eat it.
    pub fn seed_state_file(&self) -> PathBuf {
        self.base.join("cache").join("seed.json")
    }

    /// Community Store state (`store/`): install receipts, the cached
    /// catalog, the install journal, staging and backups.
    ///
    /// Outside `plugins/` (a staged folder there would be scanned as a
    /// plugin and reload on every written file) and outside `data/` (the
    /// orphan sweep would eat it).
    pub fn store_dir(&self) -> PathBuf {
        self.base.join("store")
    }

    /// What the store installed (`store/installed.json`); without an entry
    /// here a plugin folder is the user's own and is never replaced.
    pub fn store_receipts_file(&self) -> PathBuf {
        self.store_dir().join("installed.json")
    }

    /// The last catalog that verified (`store/catalog.json`), byte for byte.
    pub fn store_catalog_file(&self) -> PathBuf {
        self.store_dir().join("catalog.json")
    }

    /// Detached minisign signature of the cached catalog.
    pub fn store_catalog_signature_file(&self) -> PathBuf {
        self.store_dir().join("catalog.json.sig")
    }

    /// ETag and fetch time of the cached catalog (`store/catalog.meta.json`).
    pub fn store_catalog_meta_file(&self) -> PathBuf {
        self.store_dir().join("catalog.meta.json")
    }

    /// The in-flight install (`store/transaction.json`), replayed at startup
    /// so a crash between the two renames of a swap loses nothing.
    pub fn store_journal_file(&self) -> PathBuf {
        self.store_dir().join("transaction.json")
    }

    /// Downloads and extracted folders before they are swapped into place.
    pub fn store_staging_dir(&self) -> PathBuf {
        self.store_dir().join("staging")
    }

    /// The previous version of one plugin (`store/backups/<id>/`), kept
    /// until the next update replaces it or the plugin is removed.
    pub fn store_backup_dir(&self, plugin_id: &str) -> PathBuf {
        self.store_dir().join("backups").join(plugin_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_live_under_the_base_dir() {
        let paths = SmabarPaths::new(PathBuf::from("/tmp/base"));
        assert_eq!(paths.base_dir(), Path::new("/tmp/base"));
        assert_eq!(paths.config_file(), PathBuf::from("/tmp/base/config.json"));
        assert_eq!(paths.legal_file(), PathBuf::from("/tmp/base/legal.json"));
        assert_eq!(paths.locales_dir(), PathBuf::from("/tmp/base/locales"));
        assert_eq!(paths.logs_dir(), PathBuf::from("/tmp/base/logs"));
        assert_eq!(paths.plugins_dir(), PathBuf::from("/tmp/base/plugins"));
        assert_eq!(paths.themes_dir(), PathBuf::from("/tmp/base/themes"));
        assert_eq!(paths.tools_dir(), PathBuf::from("/tmp/base/tools"));
        assert_eq!(paths.data_dir(), PathBuf::from("/tmp/base/data"));
        assert_eq!(
            paths.plugin_data_dir("clock"),
            PathBuf::from("/tmp/base/data/clock")
        );
        assert_eq!(paths.icons_dir(), PathBuf::from("/tmp/base/cache/icons"));
        assert_eq!(paths.fonts_dir(), PathBuf::from("/tmp/base/cache/fonts"));
        assert_eq!(
            paths.google_fonts_dir(),
            PathBuf::from("/tmp/base/cache/fonts/google")
        );
        assert_eq!(
            paths.updates_dir(),
            PathBuf::from("/tmp/base/cache/updates")
        );
        assert_eq!(
            paths.seed_state_file(),
            PathBuf::from("/tmp/base/cache/seed.json")
        );
        assert_eq!(paths.store_dir(), PathBuf::from("/tmp/base/store"));
        assert_eq!(
            paths.store_receipts_file(),
            PathBuf::from("/tmp/base/store/installed.json")
        );
        assert_eq!(
            paths.store_catalog_file(),
            PathBuf::from("/tmp/base/store/catalog.json")
        );
        assert_eq!(
            paths.store_catalog_signature_file(),
            PathBuf::from("/tmp/base/store/catalog.json.sig")
        );
        assert_eq!(
            paths.store_catalog_meta_file(),
            PathBuf::from("/tmp/base/store/catalog.meta.json")
        );
        assert_eq!(
            paths.store_journal_file(),
            PathBuf::from("/tmp/base/store/transaction.json")
        );
        assert_eq!(
            paths.store_staging_dir(),
            PathBuf::from("/tmp/base/store/staging")
        );
        assert_eq!(
            paths.store_backup_dir("hello"),
            PathBuf::from("/tmp/base/store/backups/hello")
        );
    }

    /// Store state must neither be scanned as a plugin nor swept as orphaned
    /// plugin data.
    #[test]
    fn store_state_never_lives_inside_plugins_or_data() {
        let paths = SmabarPaths::new(PathBuf::from("/tmp/base"));
        for path in [
            paths.store_staging_dir(),
            paths.store_backup_dir("hello"),
            paths.store_receipts_file(),
        ] {
            assert!(!path.starts_with(paths.plugins_dir()));
            assert!(!path.starts_with(paths.data_dir()));
        }
    }

    /// The whole point of the split: a plugin writing state must not land
    /// inside the watched code folder.
    #[test]
    fn plugin_data_never_lives_inside_the_watched_plugin_folder() {
        let paths = SmabarPaths::new(PathBuf::from("/tmp/base"));
        assert!(
            !paths
                .plugin_data_dir("clock")
                .starts_with(paths.plugins_dir())
        );
    }
}
