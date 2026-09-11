//! Start Menu roots and `.lnk` inspection on the shell STA worker.

use std::ffi::{OsStr, OsString, c_void};
use std::fs;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonStartMenu, FOLDERID_StartMenu, IShellLinkW, KF_FLAG_DEFAULT,
    SHGetKnownFolderPath, SLR_NO_UI, SLR_NOSEARCH, SLR_NOTRACK, SLR_NOUPDATE, ShellLink,
};
use windows::core::{Interface as _, PCWSTR};

use crate::shortcuts::ShortcutError;
use crate::shortcuts::platform::{AppSource, IconTarget, InstalledApp, LaunchTarget};

use super::windows_error;
use super::windows_model::{StartMenuCandidate, deduplicate_start_menu};

const BUFFER_LEN: usize = 32_768;

pub(super) struct LinkInfo {
    pub target: PathBuf,
    pub arguments: String,
    pub working_dir: PathBuf,
}

pub(super) fn discover() -> Result<Vec<InstalledApp>, ShortcutError> {
    let mut candidates = Vec::new();
    let mut roots_scanned = 0_u8;
    for (folder, name) in [
        (&FOLDERID_StartMenu, "user Start Menu"),
        (&FOLDERID_CommonStartMenu, "common Start Menu"),
    ] {
        let root = match known_folder_path(folder) {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!(%error, folder = name, "Start Menu root unavailable; app search may be incomplete");
                continue;
            }
        };
        let mut links = Vec::new();
        if let Err(error) = collect_links(&root, &mut links) {
            tracing::warn!(path = %root.display(), %error, folder = name, "Start Menu root cannot be read; app search may be incomplete");
            continue;
        }
        roots_scanned += 1;
        for link in links {
            match resolve_link(&link) {
                Ok(info) if info.target.is_absolute() && info.target.exists() => {
                    let Some(label) = link
                        .file_stem()
                        .or_else(|| info.target.file_stem())
                        .map(|name| name.to_string_lossy().into_owned())
                        .filter(|name| !name.is_empty())
                    else {
                        continue;
                    };
                    candidates.push(StartMenuCandidate {
                        link,
                        target: info.target,
                        arguments: info.arguments,
                        working_dir: info.working_dir,
                        label,
                    });
                }
                Ok(_) => tracing::debug!(
                    path = %link.display(),
                    "skipping Start Menu link without an existing filesystem target; UWP links are out of scope"
                ),
                Err(error) => tracing::debug!(
                    path = %link.display(),
                    %error,
                    "skipping unsupported Start Menu link; only filesystem-backed .lnk targets are supported"
                ),
            }
        }
    }
    if roots_scanned == 0 {
        return Err(ShortcutError::Platform {
            action: "find either Windows Start Menu root",
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "both FOLDERID_StartMenu and FOLDERID_CommonStartMenu failed",
            ),
        });
    }

    Ok(deduplicate_start_menu(candidates)
        .into_iter()
        .map(|candidate| InstalledApp {
            source: AppSource::Path(candidate.link.clone()),
            name: candidate.label.clone(),
            comment: None,
            icon: Some(IconTarget::Path(candidate.link.clone())),
            launch: LaunchTarget::Path {
                name: candidate.label,
                path: candidate.link,
            },
        })
        .collect())
}

pub(super) fn resolve_link(path: &Path) -> Result<LinkInfo, ShortcutError> {
    // SAFETY: COM is initialized on the owning STA; all returned interfaces
    // stay on this thread and the input path is NUL-terminated.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| windows_error("create a Shell Link COM object", error))?;
        let persist: IPersistFile = link
            .cast()
            .map_err(|error| windows_error("query IPersistFile for a .lnk", error))?;
        let path_wide = wide(path.as_os_str());
        persist
            .Load(PCWSTR(path_wide.as_ptr()), STGM_READ)
            .map_err(|error| windows_error("load a .lnk file", error))?;

        let resolve_flags = (SLR_NO_UI | SLR_NOUPDATE | SLR_NOTRACK | SLR_NOSEARCH).0 as u32;
        if let Err(error) = link.Resolve(HWND::default(), resolve_flags) {
            tracing::debug!(
                path = %path.display(),
                %error,
                "Shell Link resolution hint failed; reading the stored filesystem target"
            );
        }

        let mut target = vec![0_u16; BUFFER_LEN];
        link.GetPath(&mut target, std::ptr::null_mut(), 0)
            .map_err(|error| windows_error("read a .lnk target", error))?;
        let target = PathBuf::from(os_string_from_buffer(&target));
        if target.as_os_str().is_empty() {
            return Err(ShortcutError::Platform {
                action: "read a filesystem target from a .lnk",
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "the link has no filesystem target",
                ),
            });
        }

        let mut arguments = vec![0_u16; BUFFER_LEN];
        link.GetArguments(&mut arguments)
            .map_err(|error| windows_error("read .lnk arguments", error))?;
        let mut working_dir = vec![0_u16; BUFFER_LEN];
        link.GetWorkingDirectory(&mut working_dir)
            .map_err(|error| windows_error("read a .lnk working directory", error))?;
        Ok(LinkInfo {
            target,
            arguments: os_string_from_buffer(&arguments)
                .to_string_lossy()
                .into_owned(),
            working_dir: PathBuf::from(os_string_from_buffer(&working_dir)),
        })
    }
}

fn known_folder_path(id: &windows::core::GUID) -> Result<PathBuf, ShortcutError> {
    // SAFETY: SHGetKnownFolderPath allocates with the COM task allocator; the
    // pointer is converted before CoTaskMemFree and never used afterwards.
    unsafe {
        let raw = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)
            .map_err(|error| windows_error("resolve a Windows known folder", error))?;
        let result = raw
            .to_string()
            .map(PathBuf::from)
            .map_err(|error| windows_error("decode a Windows known-folder path", error.into()));
        CoTaskMemFree(Some(raw.0.cast::<c_void>()));
        result
    }
}

fn collect_links(dir: &Path, links: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::debug!(path = %dir.display(), %error, "cannot read a Start Menu entry");
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            if let Err(error) = collect_links(&path, links) {
                tracing::debug!(path = %path.display(), %error, "cannot read a nested Start Menu directory");
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            links.push(path);
        }
    }
    Ok(())
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn os_string_from_buffer(buffer: &[u16]) -> OsString {
    let end = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    OsString::from_wide(&buffer[..end])
}
