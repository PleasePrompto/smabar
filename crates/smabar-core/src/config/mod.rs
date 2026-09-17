//! Config schema, atomic load/save, hot-reload watching, and dotted-path
//! updates.
//!
//! The on-disk format is pretty-printed camelCase JSON at
//! [`SmabarPaths::config_file`]. [`ConfigWatcher`] reloads on external edits
//! (debounced, with write-echo suppression) and broadcasts [`ConfigChange`]s.

mod appearance;
mod audio;
pub use audio::{AudioConfig, AudioLevel};
mod model;
mod monitor;
mod paths;
mod sections;
pub mod update;
mod validation;
mod watcher;

pub use appearance::{AppearanceConfig, BarChrome, PluginAccent, TileChrome, ZoneAlign};
pub use model::{
    ConfigError, McpConfig, RenderingMode, SettingsWindowConfig, SmabarConfig, ZOrder,
};
pub use monitor::MonitorPreference;
pub use paths::SmabarPaths;
pub use sections::{
    BarPosition, BarVariant, BarWidth, EffectsConfig, HoverMagnify, HoverPeek, LabelMode,
    LayoutBehavior, LayoutConfig, PopupPosition, PopupsConfig, ShortcutEntry, ShortcutsConfig,
    SpecialShortcut, ZoneKind,
};
pub use validation::ConfigValidationError;
pub use watcher::{ConfigChange, ConfigWatcher};
