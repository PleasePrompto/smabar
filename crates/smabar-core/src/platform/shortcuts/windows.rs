//! Windows adapter backed by one shell STA worker.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::config::SpecialShortcut;
use crate::shortcuts::platform::{
    IconTarget, InstalledApp, LaunchTarget, PlatformItem, ShortcutPlatformOps,
};
use crate::shortcuts::{ShortcutError, ShortcutPlatform};

use super::windows_discovery;
use super::windows_icons;
use super::windows_model::{is_supported_windows_path, shell_execute_error_message};

struct WindowsShortcuts {
    requests: Sender<Request>,
}

enum Request {
    Discover(Sender<Result<Vec<InstalledApp>, ShortcutError>>),
    Icon {
        target: IconTarget,
        reply: Sender<Result<Option<String>, ShortcutError>>,
    },
    Launch {
        target: LaunchTarget,
        reply: Sender<Result<(), ShortcutError>>,
    },
    OpenUrl {
        url: String,
        reply: Sender<Result<(), ShortcutError>>,
    },
}

/// Starts the worker once. Dropping every service clone closes its channel;
/// the detached thread then uninitializes COM and exits by itself.
pub fn new(icons_dir: PathBuf) -> std::io::Result<ShortcutPlatform> {
    let (requests, receiver) = channel();
    let (ready_tx, ready_rx) = channel();
    let _worker = std::thread::Builder::new()
        .name("smabar-shortcuts".to_string())
        .spawn(move || worker(receiver, ready_tx, icons_dir))?;
    ready_rx
        .recv()
        .map_err(|_| std::io::Error::other("Windows shortcut worker stopped during startup"))??;
    Ok(ShortcutPlatform::new(WindowsShortcuts { requests }))
}

impl ShortcutPlatformOps for WindowsShortcuts {
    fn discover_apps(&self) -> Result<Vec<InstalledApp>, ShortcutError> {
        self.request("scan the Windows Start Menu", Request::Discover)
    }

    fn inspect_path(&self, path: &Path) -> Result<PlatformItem, ShortcutError> {
        if !is_supported_windows_path(path) {
            return Err(ShortcutError::InvalidDesktopPath {
                path: path.to_path_buf(),
            });
        }
        let label = path_label(path, path.is_dir());
        Ok(PlatformItem {
            label: label.clone(),
            icon: Some(IconTarget::Path(path.to_path_buf())),
            launch: LaunchTarget::Path {
                name: label,
                path: path.to_path_buf(),
            },
        })
    }

    fn resolve_icon(&self, target: &IconTarget) -> Result<Option<String>, ShortcutError> {
        self.request("extract a Windows shortcut icon", |reply| Request::Icon {
            target: target.clone(),
            reply,
        })
    }

    fn launch(&self, target: &LaunchTarget) -> Result<(), ShortcutError> {
        self.request("open a Windows shortcut", |reply| Request::Launch {
            target: target.clone(),
            reply,
        })
    }

    fn special(&self, special: SpecialShortcut) -> PlatformItem {
        let label = match special {
            SpecialShortcut::Computer => "This PC",
            SpecialShortcut::Trash => "Recycle Bin",
        };
        PlatformItem {
            label: label.to_string(),
            icon: Some(IconTarget::Special(special)),
            launch: LaunchTarget::Special(special),
        }
    }

    fn open_url(&self, url: &str) -> Result<(), ShortcutError> {
        self.request("open the default Windows browser", |reply| {
            Request::OpenUrl {
                url: url.to_string(),
                reply,
            }
        })
    }
}

impl WindowsShortcuts {
    fn request<T>(
        &self,
        action: &'static str,
        make_request: impl FnOnce(Sender<Result<T, ShortcutError>>) -> Request,
    ) -> Result<T, ShortcutError> {
        let (reply_tx, reply_rx) = channel();
        self.requests
            .send(make_request(reply_tx))
            .map_err(|_| worker_stopped(action))?;
        reply_rx.recv().map_err(|_| worker_stopped(action))?
    }
}

fn worker(receiver: Receiver<Request>, ready: Sender<std::io::Result<()>>, icons_dir: PathBuf) {
    // SAFETY: this thread owns one STA for its whole lifetime and every COM
    // interface is created, used and dropped inside it.
    let initialized =
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok() };
    if let Err(error) = initialized {
        let _ = ready.send(Err(std::io::Error::other(error)));
        return;
    }
    if ready.send(Ok(())).is_err() {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
        return;
    }

    for request in receiver {
        match request {
            Request::Discover(reply) => {
                let _ = reply.send(windows_discovery::discover());
            }
            Request::Icon { target, reply } => {
                let _ = reply.send(windows_icons::resolve(&target, &icons_dir));
            }
            Request::Launch { target, reply } => {
                let _ = reply.send(open_target(&target));
            }
            Request::OpenUrl { url, reply } => {
                let result = shell_execute(OsStr::new(&url), None).map(|()| {
                    tracing::info!("opened link in the default Windows browser");
                });
                let _ = reply.send(result);
            }
        }
    }

    // SAFETY: balances this worker's successful CoInitializeEx.
    unsafe { CoUninitialize() };
}

fn open_target(target: &LaunchTarget) -> Result<(), ShortcutError> {
    match target {
        LaunchTarget::Path { name, path } => {
            shell_execute(path.as_os_str(), None)?;
            tracing::info!(item = name, path = %path.display(), "opened Windows shortcut target");
            Ok(())
        }
        LaunchTarget::Special(special) => {
            let argument = match special {
                SpecialShortcut::Computer => "shell:MyComputerFolder",
                SpecialShortcut::Trash => "shell:RecycleBinFolder",
            };
            shell_execute(OsStr::new("explorer.exe"), Some(OsStr::new(argument)))?;
            tracing::info!(special = ?special, "opened Windows special shortcut");
            Ok(())
        }
        LaunchTarget::Desktop { .. } => Err(ShortcutError::Platform {
            action: "launch an XDG desktop entry on Windows",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "desktop-entry launch data reached the Windows adapter",
            ),
        }),
    }
}

fn shell_execute(file: &OsStr, parameters: Option<&OsStr>) -> Result<(), ShortcutError> {
    let file = wide(file);
    let parameters = parameters.map(wide);
    let parameter_ptr = parameters
        .as_ref()
        .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    // SAFETY: every string is NUL-terminated and lives until the call returns;
    // the fixed "open" verb delegates association handling to the Shell.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file.as_ptr()),
            parameter_ptr,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code > 32 {
        return Ok(());
    }
    Err(ShortcutError::Platform {
        action: "open an item through the Windows Shell",
        source: std::io::Error::other(shell_execute_error_message(code)),
    })
}

fn worker_stopped(action: &'static str) -> ShortcutError {
    ShortcutError::Platform {
        action,
        source: std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Windows shortcut worker is no longer running; restart smabar",
        ),
    }
}

fn path_label(path: &Path, is_directory: bool) -> String {
    let name = if is_directory {
        path.file_name()
    } else {
        path.file_stem()
    };
    name.map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
