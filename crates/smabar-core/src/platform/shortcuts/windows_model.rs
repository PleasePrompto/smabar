//! Pure Windows shortcut helpers kept runnable on every development host.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A Start Menu link after COM resolved its launch target and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartMenuCandidate {
    pub link: PathBuf,
    pub target: PathBuf,
    pub arguments: String,
    pub working_dir: PathBuf,
    pub label: String,
}

/// User-menu candidates are passed first and therefore win duplicates.
pub(super) fn deduplicate_start_menu(
    candidates: Vec<StartMenuCandidate>,
) -> Vec<StartMenuCandidate> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for candidate in candidates {
        let key = format!(
            "{}\0{}\0{}",
            candidate.target.to_string_lossy().to_lowercase(),
            candidate.arguments,
            candidate.working_dir.to_string_lossy().to_lowercase()
        );
        if seen.insert(key) {
            result.push(candidate);
        }
    }
    result.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.link.cmp(&right.link))
    });
    result
}

/// Windows Shell paths must name an existing absolute file or directory.
pub(super) fn is_supported_windows_path(path: &Path) -> bool {
    path.is_absolute() && (path.is_file() || path.is_dir())
}

/// Stable, non-cryptographic cache name for one native icon source.
pub(super) fn icon_cache_name(key: &str) -> String {
    // FNV-1a: deterministic across runs/toolchains; collision resistance is
    // enough for a cosmetic cache whose entries can always be regenerated.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("native-{hash:016x}.png")
}

/// Converts Shell BGRA pixels to RGBA. Some GDI bitmaps omit alpha entirely;
/// Explorer treats those pixels as opaque.
pub(super) fn normalize_bgra_pixels(pixels: &mut [u8]) {
    let missing_alpha = !pixels.chunks_exact(4).any(|pixel| pixel[3] != 0);
    for pixel in pixels.chunks_exact_mut(4) {
        if missing_alpha {
            pixel[3] = 255;
        }
        pixel.swap(0, 2);
    }
}

pub(super) fn shell_execute_error_message(code: isize) -> String {
    let message = match code {
        0 => "Windows is out of memory or resources",
        2 => "the file was not found",
        3 => "the path was not found",
        5 => "access was denied",
        8 => "Windows does not have enough memory",
        11 => "the executable format is invalid",
        26 => "a sharing violation occurred",
        27 => "the file association is incomplete or invalid",
        28 => "the DDE request timed out",
        29 => "the DDE transaction failed",
        30 => "the DDE server is busy",
        31 => "no application is associated with this item",
        32 => "a required DLL was not found",
        _ => return format!("ShellExecuteW failed with code {code}"),
    };
    message.to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn windows_path_policy_accepts_every_existing_absolute_file_or_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = dir.path().join("App.LNK");
        let exe = dir.path().join("tool.ExE");
        let text = dir.path().join("notes.txt");
        fs::write(&link, b"link").expect("link");
        fs::write(&exe, b"exe").expect("exe");
        fs::write(&text, b"text").expect("text");

        assert!(is_supported_windows_path(dir.path()));
        assert!(is_supported_windows_path(&link));
        assert!(is_supported_windows_path(&exe));
        assert!(is_supported_windows_path(&text));
        assert!(!is_supported_windows_path(&dir.path().join("missing.pdf")));
        assert!(!is_supported_windows_path(Path::new("relative.lnk")));
    }

    #[test]
    fn start_menu_dedup_uses_target_and_arguments_then_sorts() {
        let candidate =
            |link: &str, target: &str, arguments: &str, working_dir: &str, label: &str| {
                StartMenuCandidate {
                    link: PathBuf::from(link),
                    target: PathBuf::from(target),
                    arguments: arguments.to_string(),
                    working_dir: PathBuf::from(working_dir),
                    label: label.to_string(),
                }
            };
        let apps = deduplicate_start_menu(vec![
            candidate("user-z.lnk", "C:/Blender.exe", "", "", "Z Blender"),
            candidate("common.lnk", "c:/blender.EXE", "", "", "Duplicate"),
            candidate(
                "profile-upper.lnk",
                "C:/Blender.exe",
                "--profile Foo",
                "C:/profiles",
                "A Blender",
            ),
            candidate(
                "profile-lower.lnk",
                "C:/Blender.exe",
                "--profile foo",
                "C:/profiles",
                "B Blender",
            ),
            candidate(
                "other-workdir.lnk",
                "C:/Blender.exe",
                "",
                "C:/other",
                "C Blender",
            ),
        ]);
        let links: Vec<&Path> = apps.iter().map(|app| app.link.as_path()).collect();
        assert_eq!(
            links,
            vec![
                Path::new("profile-upper.lnk"),
                Path::new("profile-lower.lnk"),
                Path::new("other-workdir.lnk"),
                Path::new("user-z.lnk")
            ]
        );
    }

    #[test]
    fn native_icon_cache_name_is_stable_and_namespaced() {
        assert_eq!(
            icon_cache_name("path:C:/Program Files/App/app.exe:256"),
            "native-d115192aee5ad04c.png"
        );
        assert_ne!(icon_cache_name("computer"), icon_cache_name("trash"));
    }

    #[test]
    fn native_pixels_repair_missing_alpha_and_preserve_straight_alpha() {
        let mut opaque = [10, 20, 30, 0];
        normalize_bgra_pixels(&mut opaque);
        assert_eq!(opaque, [30, 20, 10, 255]);

        let mut translucent = [32, 16, 8, 128];
        normalize_bgra_pixels(&mut translucent);
        assert_eq!(translucent, [8, 16, 32, 128]);
    }

    #[test]
    fn shell_execute_errors_use_the_shell_error_domain() {
        assert_eq!(
            shell_execute_error_message(31),
            "no application is associated with this item"
        );
        assert_eq!(
            shell_execute_error_message(99),
            "ShellExecuteW failed with code 99"
        );
    }
}
