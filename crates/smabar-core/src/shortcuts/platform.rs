//! Neutral shortcut items and the injected platform boundary.
//!
//! The service owns source validation and pin orchestration. Native discovery,
//! icon lookup and opening stay behind this handle, whose implementations live
//! under `crate::platform::shortcuts`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::SpecialShortcut;

use super::ShortcutError;

/// The source identifier returned by installed-application discovery.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppSource {
    /// XDG desktop-file id on Linux and compatible Unix desktops.
    DesktopId(String),
    /// Absolute Start Menu `.lnk` path on Windows or `.app` bundle on macOS.
    Path(PathBuf),
}

impl AppSource {
    pub(crate) fn id(&self) -> String {
        match self {
            Self::DesktopId(id) => id.clone(),
            Self::Path(path) => path.to_string_lossy().into_owned(),
        }
    }
}

/// One installed application returned to the Tauri and MCP frontends.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub source: AppSource,
    pub name: String,
    pub comment: Option<String>,
    pub(crate) icon: Option<IconTarget>,
    pub(crate) launch: LaunchTarget,
}

/// A platform-resolved icon request. It is also the in-memory cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum IconTarget {
    /// Theme icon names in platform preference order, including fallbacks.
    Theme(Vec<String>),
    Path(PathBuf),
    Special(SpecialShortcut),
}

/// A launch target whose contents came from a validated platform source.
#[derive(Debug, Clone)]
pub(crate) enum LaunchTarget {
    Desktop {
        name: String,
        exec: String,
        working_dir: Option<PathBuf>,
        terminal: bool,
    },
    Path {
        name: String,
        path: PathBuf,
    },
    Special(SpecialShortcut),
}

/// Label, icon and launch target of an explicit path or special item.
#[derive(Debug, Clone)]
pub(crate) struct PlatformItem {
    pub label: String,
    pub icon: Option<IconTarget>,
    pub launch: LaunchTarget,
}

pub(crate) trait ShortcutPlatformOps: Send + Sync {
    fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError>;
    fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError>;
    fn resolve_icon(&self, target: &IconTarget) -> Result<Option<String>, ShortcutError>;
    fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError>;
    fn special(&self, special: SpecialShortcut) -> PlatformItem;
    fn open_url(&self, url: &str) -> Result<(), ShortcutError>;
}

/// Cheaply clonable, opaque platform behavior injected by the app edge.
#[derive(Clone)]
pub struct ShortcutPlatform {
    operations: Arc<dyn ShortcutPlatformOps>,
}

impl ShortcutPlatform {
    pub(crate) fn new(operations: impl ShortcutPlatformOps + 'static) -> Self {
        Self {
            operations: Arc::new(operations),
        }
    }

    pub(crate) fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
        self.operations.discover_apps()
    }

    pub(crate) fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError> {
        self.operations.inspect_path(path)
    }

    pub(crate) fn resolve_icon(
        &self,
        target: &IconTarget,
    ) -> Result<Option<String>, ShortcutError> {
        self.operations.resolve_icon(target)
    }

    pub(crate) fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
        self.operations.launch(target)
    }

    pub(crate) fn special(&self, special: SpecialShortcut) -> PlatformItem {
        self.operations.special(special)
    }

    pub(crate) fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
        self.operations.open_url(url)
    }
}
