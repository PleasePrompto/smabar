//! Shortcut platform implementations selected and injected by the app edge.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::new as macos;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_discovery;
#[cfg(windows)]
mod windows_icons;
#[cfg(any(windows, test))]
mod windows_model;
mod xdg;

#[cfg(windows)]
pub use windows::new as windows;
pub use xdg::new as xdg;

#[cfg(windows)]
fn windows_error(
    action: &'static str,
    source: ::windows::core::Error,
) -> crate::shortcuts::ShortcutError {
    crate::shortcuts::ShortcutError::Platform {
        action,
        source: std::io::Error::other(source),
    }
}
