//! Whether a listed item can be installed on this machine, and how versions
//! compare — SemVer precedence only, build metadata never decides.

use semver::Version;
use serde::Serialize;

/// One reason a listed item is not installable here. The shell turns each
/// into a sentence; nothing here is user-facing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Incompatibility {
    /// `requires.os` does not name this operating system.
    Os,
    /// `requires.smabar` is newer than this smabar.
    MinSmabar,
    /// The id belongs to a plugin that ships with smabar.
    BasePlugin,
    /// A folder or file with this id exists without an install receipt: it
    /// is the user's own, and the store never replaces it.
    UserPlugin,
    /// The store blocked the listed version.
    Blocked,
    /// The name belongs to a theme compiled into smabar.
    BundledTheme,
}

/// Whether this smabar (`app`) satisfies a listed minimum version.
///
/// An unparsable minimum counts as not satisfied: the listing is the store's
/// validated word, so a value this client cannot read means a newer contract.
pub fn smabar_satisfies(app: &Version, minimum: Option<&str>) -> bool {
    match minimum {
        None => true,
        Some(minimum) => Version::parse(minimum.trim())
            .map(|minimum| app.cmp_precedence(&minimum).is_ge())
            .unwrap_or(false),
    }
}

/// Whether `candidate` is a newer version than `installed`.
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    match (
        Version::parse(candidate.trim()),
        Version::parse(installed.trim()),
    ) {
        (Ok(candidate), Ok(installed)) => candidate.cmp_precedence(&installed).is_gt(),
        _ => false,
    }
}

/// Whether `requires.os` allows `host`.
pub fn os_allowed(required: &[String], host: Option<&str>) -> bool {
    host.is_some_and(|host| required.iter().any(|os| os == host))
}
