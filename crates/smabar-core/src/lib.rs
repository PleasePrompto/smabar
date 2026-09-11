//! Tauri-free core library for smabar.
//!
//! Platform glue that needs Tauri/GTK types lives in the `smabar` crate;
//! everything here is testable without a window.

pub mod capture;
pub mod config;
pub mod embed;
pub mod fonts;
pub mod i18n;
pub mod legal;
pub mod logging;
pub mod mcp;
pub mod open;
pub mod platform;
pub mod plugins;
pub mod providers;
pub mod shortcuts;
pub mod store;
pub mod themes;
mod util;
