use semver::Version;

use super::compat::{is_newer, os_allowed, smabar_satisfies};

fn app(version: &str) -> Version {
    Version::parse(version).expect("semver")
}

#[test]
fn minimum_version_is_compared_by_precedence() {
    assert!(smabar_satisfies(&app("0.1.1"), None));
    assert!(smabar_satisfies(&app("0.1.1"), Some("0.1.0")));
    assert!(smabar_satisfies(&app("0.1.1"), Some("0.1.1")));
    assert!(!smabar_satisfies(&app("0.1.1"), Some("0.2.0")));
    // Build metadata never decides; a pre-release of the app is older.
    assert!(smabar_satisfies(&app("1.0.0+build.5"), Some("1.0.0")));
    assert!(!smabar_satisfies(&app("1.0.0-beta.1"), Some("1.0.0")));
    assert!(!smabar_satisfies(&app("1.0.0"), Some("not a version")));
}

#[test]
fn newer_means_strictly_greater_precedence() {
    assert!(is_newer("0.1.1", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.1.1"));
    assert!(!is_newer("0.1.0+a", "0.1.0+b"));
    assert!(!is_newer("garbage", "0.1.0"));
}

#[test]
fn the_host_os_must_be_named() {
    let both = vec!["linux".to_string(), "windows".to_string()];
    assert!(os_allowed(&both, Some("linux")));
    assert!(!os_allowed(&["windows".to_string()], Some("linux")));
    assert!(!os_allowed(&both, None));
}
