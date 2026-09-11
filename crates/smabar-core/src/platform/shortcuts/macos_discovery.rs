//! Scan normal macOS application folders without descending into app bundles.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSNumber, NSString};

use crate::shortcuts::ShortcutError;
use crate::shortcuts::platform::{AppSource, IconTarget, InstalledApp, LaunchTarget};

pub(super) fn discover(home: &Path) -> Result<Vec<InstalledApp>, ShortcutError> {
    let roots = [
        home.join("Applications"),
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ];
    let mut apps = scan(&roots)?;
    // Finder is an ordinary launchable app stored outside the application folders.
    if let Some(url) = NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str("com.apple.finder"))
        && let Some(path) = url.to_file_path()
        && !apps
            .iter()
            .any(|app| app.source == AppSource::Path(path.clone()))
        && let Some(app) = application(&path)
    {
        apps.push(app);
    }
    Ok(apps)
}

fn scan(roots: &[PathBuf]) -> Result<Vec<InstalledApp>, ShortcutError> {
    let mut pending = roots.to_vec();
    let mut visited = HashSet::new();
    let mut apps = Vec::new();
    while let Some(path) = pending.pop() {
        let canonical = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "cannot inspect macOS application folder");
                continue;
            }
        };
        if !visited.insert(canonical) || !path.is_dir() {
            continue;
        }
        if is_app(&path) {
            if let Some(app) = application(&path) {
                apps.push(app);
            }
            continue;
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                tracing::warn!(%error, path = %path.display(), "cannot read macOS application folder; app search may be incomplete");
                continue;
            }
            Err(source) => {
                return Err(ShortcutError::Platform {
                    action: "scan macOS applications",
                    source,
                });
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| ShortcutError::Platform {
                action: "scan macOS applications",
                source,
            })?;
            if !entry.file_name().to_string_lossy().starts_with('.') {
                pending.push(entry.path());
            }
        }
    }
    Ok(apps)
}

pub(super) fn is_app(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

pub(super) fn application(path: &Path) -> Option<InstalledApp> {
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    // Finder uses FNDR instead of the ordinary APPL bundle type.
    if !matches!(
        text(&bundle, "CFBundlePackageType").as_deref(),
        Some("APPL" | "FNDR")
    ) || flag(&bundle, "LSBackgroundOnly")
        || !bundle.executableURL()?.to_file_path()?.is_file()
    {
        return None;
    }
    let name = text(&bundle, "CFBundleDisplayName")
        .or_else(|| text(&bundle, "CFBundleName"))
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    Some(InstalledApp {
        source: AppSource::Path(path.to_path_buf()),
        comment: None,
        icon: Some(IconTarget::Path(path.to_path_buf())),
        launch: LaunchTarget::Path {
            name: name.clone(),
            path: path.to_path_buf(),
        },
        name,
    })
}

fn text(bundle: &NSBundle, key: &str) -> Option<String> {
    bundle
        .objectForInfoDictionaryKey(&NSString::from_str(key))?
        .downcast::<NSString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
}

fn flag(bundle: &NSBundle, key: &str) -> bool {
    bundle
        .objectForInfoDictionaryKey(&NSString::from_str(key))
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .is_some_and(|value| value.boolValue())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::platform::ShortcutPlatformOps;
    use objc2::rc::autoreleasepool;

    fn bundle(root: &Path, name: &str, background: bool) -> PathBuf {
        let app = root.join(format!("{name}.app"));
        let contents = app.join("Contents");
        fs::create_dir_all(contents.join("MacOS")).expect("bundle directories");
        fs::write(contents.join("MacOS/program"), "#!/bin/sh\nexit 0\n").expect("executable");
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0"?>
<plist version="1.0"><dict>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleExecutable</key><string>program</string>
<key>CFBundleDisplayName</key><string>Native Test App</string>
<key>LSBackgroundOnly</key><{} />
<key>LSUIElement</key><true />
</dict></plist>"#,
                if background { "true" } else { "false" }
            ),
        )
        .expect("bundle plist");
        app
    }

    #[test]
    fn discovers_pin_ready_bundles_without_helpers_duplicates_or_symlink_loops() {
        autoreleasepool(|_| {
            let temp = tempfile::tempdir().expect("temp directory");
            let root = temp.path().join("Applications");
            let app = bundle(&root.join("Utilities"), "Names + spaces", false);
            bundle(&app.join("Contents/Helpers"), "Hidden Helper", false);
            bundle(&root, "Background Agent", true);
            fs::create_dir_all(root.join("Broken.app")).expect("broken bundle");
            std::os::unix::fs::symlink(&root, root.join("cycle")).expect("directory cycle");
            let apps = scan(&[root.clone(), root]).expect("scan");
            assert_eq!(apps.len(), 1);
            assert_eq!(apps[0].name, "Native Test App");
            assert_eq!(apps[0].source, AppSource::Path(app.clone()));
            let finder = bundle(&temp.path().join("Applications"), "Finder", false);
            let plist = finder.join("Contents/Info.plist");
            let content = fs::read_to_string(&plist).expect("Finder plist");
            fs::write(&plist, content.replace("APPL", "FNDR")).expect("Finder bundle type");
            assert!(application(&finder).is_some(), "Finder is launchable too");
            let platform = super::super::MacShortcuts {
                home: temp.path().to_path_buf(),
            };
            let item = platform.inspect_path(&app).expect("pin app");
            assert_eq!(item.label, "Native Test App");
            assert!(matches!(item.launch, LaunchTarget::Path { path, .. } if path == app));
            assert!(platform.inspect_path(Path::new("relative.app")).is_err());
            assert!(
                platform
                    .inspect_path(&temp.path().join("Missing.app"))
                    .is_err()
            );
            assert!(platform.open_url("javascript:alert(1)").is_err());
            assert!(platform.open_url("file:///tmp/test").is_err());
        });
    }
}
