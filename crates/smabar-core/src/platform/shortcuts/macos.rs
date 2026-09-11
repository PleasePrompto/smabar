//! Native macOS shortcut opening and icons. Source validation stays in the
//! shared service; NSWorkspace receives URLs, never a shell command string.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSFileManager, NSPoint, NSRect, NSSize, NSString, NSURL};

use crate::config::SpecialShortcut;
use crate::shortcuts::ShortcutError;
use crate::shortcuts::iconfile::MAX_ICON_BYTES;
use crate::shortcuts::platform::{
    IconTarget, InstalledApp, LaunchTarget, PlatformItem, ShortcutPlatform, ShortcutPlatformOps,
};

#[path = "macos_discovery.rs"]
mod discovery;

struct MacShortcuts {
    home: PathBuf,
}

pub fn new(home: PathBuf) -> ShortcutPlatform {
    ShortcutPlatform::new(MacShortcuts { home })
}

impl ShortcutPlatformOps for MacShortcuts {
    fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
        autoreleasepool(|_| discovery::discover(&self.home))
    }

    fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError> {
        autoreleasepool(|_| {
            if !path.is_absolute() || !(path.is_file() || path.is_dir()) {
                return Err(invalid_path(path));
            }
            let label = if discovery::is_app(path) {
                discovery::application(path)
                    .ok_or_else(|| invalid_path(path))?
                    .name
            } else {
                NSFileManager::defaultManager()
                    .displayNameAtPath(&*native_path(path)?)
                    .to_string()
            };
            Ok(PlatformItem {
                label: label.clone(),
                icon: Some(IconTarget::Path(path.to_path_buf())),
                launch: LaunchTarget::Path {
                    name: label,
                    path: path.to_path_buf(),
                },
            })
        })
    }

    fn resolve_icon(&self, target: &IconTarget) -> Result<Option<String>, ShortcutError> {
        autoreleasepool(|_| {
            let path = match target {
                IconTarget::Path(path) => path.clone(),
                IconTarget::Special(special) => self.special_path(*special),
                IconTarget::Theme(_) => return Ok(None),
            };
            let icon = NSWorkspace::sharedWorkspace().iconForFile(&*native_path(&path)?);
            let mut rect = NSRect::new(NSPoint::ZERO, NSSize::new(128.0, 128.0));
            // SAFETY: rect remains valid for the synchronous call; no context or hints.
            let image = unsafe { icon.CGImageForProposedRect_context_hints(&mut rect, None, None) }
                .ok_or_else(|| native_error("render a shortcut icon", "macOS returned no image"))?;
            let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &image);
            // SAFETY: an empty properties dictionary has no incorrectly typed values.
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }
            .ok_or_else(|| native_error("encode a shortcut icon", "macOS returned no PNG"))?;
            if png.length() as u64 > MAX_ICON_BYTES {
                return Err(native_error(
                    "encode a shortcut icon",
                    "PNG exceeds the icon size limit",
                ));
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(png.to_vec());
            Ok(Some(format!("data:image/png;base64,{encoded}")))
        })
    }

    fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
        autoreleasepool(|_| match target {
            LaunchTarget::Path { path, .. } => open_path(path),
            LaunchTarget::Special(special) => open_path(&self.special_path(*special)),
            LaunchTarget::Desktop { .. } => Err(native_error(
                "open a shortcut",
                "Linux desktop entries cannot run on macOS; add the app again using app search",
            )),
        })
    }

    fn special(&self, special: SpecialShortcut) -> PlatformItem {
        PlatformItem {
            label: autoreleasepool(|_| {
                NSFileManager::defaultManager()
                    .displayNameAtPath(&NSString::from_str(
                        &self.special_path(special).to_string_lossy(),
                    ))
                    .to_string()
            }),
            icon: Some(IconTarget::Special(special)),
            launch: LaunchTarget::Special(special),
        }
    }

    fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
        crate::open::validate_url(url)?;
        autoreleasepool(|_| {
            let url = NSURL::URLWithString(&NSString::from_str(url))
                .ok_or_else(|| native_error("open a website", "macOS could not parse the URL"))?;
            open(&url, "open the website in the default browser")
        })
    }
}

impl MacShortcuts {
    fn special_path(&self, special: SpecialShortcut) -> PathBuf {
        match special {
            SpecialShortcut::Computer => PathBuf::from("/"),
            SpecialShortcut::Trash => self.home.join(".Trash"),
        }
    }
}

fn open_path(path: &Path) -> Result<(), ShortcutError> {
    if !path.is_absolute() || !(path.is_file() || path.is_dir()) {
        return Err(invalid_path(path));
    }
    let url = NSURL::from_file_path(path).ok_or_else(|| invalid_path(path))?;
    open(&url, "open the application, file or folder")
}

fn open(url: &NSURL, action: &'static str) -> Result<(), ShortcutError> {
    if NSWorkspace::sharedWorkspace().openURL(url) {
        Ok(())
    } else {
        Err(native_error(
            action,
            "macOS refused to open the item; check that the application is installed and a default app is assigned",
        ))
    }
}

fn native_path(path: &Path) -> Result<objc2::rc::Retained<NSString>, ShortcutError> {
    path.to_str()
        .map(NSString::from_str)
        .ok_or_else(|| invalid_path(path))
}

fn invalid_path(path: &Path) -> ShortcutError {
    ShortcutError::InvalidDesktopPath {
        path: path.to_path_buf(),
    }
}

fn native_error(action: &'static str, message: &str) -> ShortcutError {
    ShortcutError::Platform {
        action,
        source: std::io::Error::other(message),
    }
}
