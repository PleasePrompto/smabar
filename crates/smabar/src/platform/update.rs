//! Platform facts for application updates (ADR 0009): which `platforms` key
//! this installation looks up in `latest.json`, and how it applies a release.

use serde::Serialize;

/// How this build applies an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    /// The updater plugin applies the release itself: Windows runs the
    /// installer and ends the process, macOS swaps the bundle and restarts.
    App,
    /// The verified package is handed to the system installer (Linux).
    System,
}

/// `None` where smabar cannot apply an update at all; the shell then shows
/// the release without an install button.
pub fn install_mode() -> Option<InstallMode> {
    if cfg!(any(windows, target_os = "macos")) {
        Some(InstallMode::App)
    } else if cfg!(target_os = "linux") {
        Some(InstallMode::System)
    } else {
        None
    }
}

/// The updater's `platforms` key for this installation. Linux narrows the
/// plugin default `linux-<arch>` to the package format the system installs;
/// every other target keeps the plugin default.
pub fn updater_target() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let has_dpkg = std::path::Path::new("/var/lib/dpkg/status").is_file();
        let has_rpm = std::path::Path::new("/var/lib/rpm").is_dir();
        Some(linux_updater_target(
            has_dpkg,
            has_rpm,
            std::env::consts::ARCH,
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// `linux-deb-x86_64` or `linux-rpm-x86_64`. dpkg wins when both databases
/// exist (alien leaves an rpm database on Debian derivatives); with neither,
/// deb is the fallback so an unpackaged dev build still gets a check result.
#[cfg(any(target_os = "linux", test))]
fn linux_updater_target(has_dpkg: bool, has_rpm: bool, arch: &str) -> String {
    let format = if !has_dpkg && has_rpm { "rpm" } else { "deb" };
    format!("linux-{format}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::linux_updater_target;

    #[test]
    fn linux_target_names_the_package_format() {
        assert_eq!(
            linux_updater_target(true, false, "x86_64"),
            "linux-deb-x86_64"
        );
        assert_eq!(
            linux_updater_target(false, true, "aarch64"),
            "linux-rpm-aarch64"
        );
        assert_eq!(
            linux_updater_target(true, true, "x86_64"),
            "linux-deb-x86_64"
        );
        assert_eq!(
            linux_updater_target(false, false, "x86_64"),
            "linux-deb-x86_64"
        );
    }
}
