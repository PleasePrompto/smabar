//! Platform-neutral session and geometry types.
//!
//! Pure logic only — the GTK/Tauri glue that consumes these types lives in
//! the `smabar` crate so this library stays testable without a window.

use serde::{Deserialize, Serialize};

#[cfg_attr(not(any(target_os = "linux", windows)), path = "audio/fallback.rs")]
mod audio;
mod media;
pub mod render;
pub mod settings;
pub mod shortcuts;
pub mod surfaces;

pub(crate) use audio::{AUDIO_SUPPORTED, AudioError, AudioSource};
pub(crate) use media::{MEDIA_SUPPORTED, MediaError, MediaSource};

/// The operating system name as the Community Catalog spells it
/// (`requires.os`: `linux`, `windows`, `macos`), or `None` on a system the
/// catalog has no name for — nothing is installable there.
///
/// Read from `std::env::consts::OS` here so `store/` carries no platform
/// branch of its own.
pub fn current_os() -> Option<&'static str> {
    match std::env::consts::OS {
        os @ ("linux" | "windows" | "macos") => Some(os),
        _ => None,
    }
}

/// Marks an extracted file executable where the filesystem has such a bit.
/// Windows has none: the Community Store computes content hashes from the
/// archive's modes, never from disk, so nothing is lost there.
#[cfg(unix)]
pub(crate) fn mark_executable(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn mark_executable(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

/// The display-server session the app runs under, classified from
/// `XDG_SESSION_TYPE`. Callers read the environment and pass the value in,
/// keeping the detection deterministic and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    X11,
    Wayland,
    Other,
}

impl SessionKind {
    /// Classifies the value of `XDG_SESSION_TYPE` (`None` = variable unset).
    pub fn from_xdg_session_type(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(v) if v.eq_ignore_ascii_case("x11") => Self::X11,
            Some(v) if v.eq_ignore_ascii_case("wayland") => Self::Wayland,
            _ => Self::Other,
        }
    }
}

/// Native window integration selected for the current display session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowBackend {
    X11,
    WaylandLayerShell,
    WaylandFallback,
    Other,
}

/// Native stacking intent. A reserved `Panel` sits below application windows;
/// `Top` keeps an unreserved bar above ordinary windows while still following
/// compositor fullscreen policy. Transient overlay windows choose their
/// native layer directly and never change the bar's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLevel {
    Bottom,
    Panel,
    Top,
}

impl WindowBackend {
    /// Selects the integration without consulting the environment or GTK.
    pub fn select(
        session: SessionKind,
        layer_shell_supported: bool,
        on_demand_keyboard_supported: bool,
    ) -> Self {
        match (
            session,
            layer_shell_supported && on_demand_keyboard_supported,
        ) {
            (SessionKind::X11, _) => Self::X11,
            (SessionKind::Wayland, true) => Self::WaylandLayerShell,
            (SessionKind::Wayland, false) => Self::WaylandFallback,
            (SessionKind::Other, _) => Self::Other,
        }
    }
}

/// Puts the child of `command` into its own process group so it survives
/// smabar exiting (launched apps must never die with the bar).
///
/// This is the sanctioned home for the platform conditional: CLAUDE.md bans
/// platform `cfg`s outside `platform/`, and `process_group` only exists on
/// Unix. On other platforms this is a no-op until their launch story lands
/// with the platform work (Windows: CREATE_NEW_PROCESS_GROUP, M7).
pub fn detach_into_own_process_group(command: &mut std::process::Command) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
        true
    }
    #[cfg(not(unix))]
    {
        // Reference the parameter so non-Unix builds stay warning-free.
        let _ = command;
        false
    }
}

/// Hides the console window Windows opens for every console-subsystem child
/// (uv, python plugins). Anything the bar spawns for ITSELF goes through
/// here; apps the USER launches keep their windows and must not.
///
/// `creation_flags` REPLACES the whole flag set — when the Windows
/// process-group story lands (CREATE_NEW_PROCESS_GROUP, M7), OR the flags
/// together here instead of calling `creation_flags` twice.
pub fn configure_no_window(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        // Reference the parameter so non-Windows builds stay warning-free.
        let _ = command;
    }
}

/// Which signal [`signal_process_group`] delivers.
#[derive(Debug, Clone, Copy)]
pub enum ProcessSignal {
    /// Ask the group to exit (SIGTERM).
    Terminate,
    /// Force the group to exit (SIGKILL).
    Kill,
}

/// Signals every process in the group led by `pgid` — the only way to reach a
/// child's own children, which a plain `Child::kill` leaves running.
///
/// Returns `false` when the group is already gone or the platform has no
/// process groups. `pgid` MUST come from a process this code spawned with
/// [`detach_into_own_process_group`] returning `true`; signalling group 0
/// would hit smabar's own group.
pub fn signal_process_group(pgid: u32, signal: ProcessSignal) -> bool {
    #[cfg(unix)]
    {
        let Ok(pgid) = i32::try_from(pgid) else {
            return false;
        };
        if pgid <= 0 {
            return false;
        }
        let signal = match signal {
            ProcessSignal::Terminate => libc::SIGTERM,
            ProcessSignal::Kill => libc::SIGKILL,
        };
        // SAFETY: killpg is a plain libc call; a stale pgid returns ESRCH
        // instead of touching an unrelated group, and pgid > 0 is checked.
        unsafe { libc::killpg(pgid, signal) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = (pgid, signal);
        false
    }
}

/// Axis-aligned rectangle in logical (CSS) pixels, shared between the shell
/// (which measures DOM rects) and the window glue (which builds GTK input
/// regions from them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_x11_case_insensitively_and_trimmed() {
        for value in ["x11", "X11", " x11 ", "X11\n"] {
            assert_eq!(
                SessionKind::from_xdg_session_type(Some(value)),
                SessionKind::X11
            );
        }
    }

    #[test]
    fn detects_wayland() {
        assert_eq!(
            SessionKind::from_xdg_session_type(Some("wayland")),
            SessionKind::Wayland
        );
        assert_eq!(
            SessionKind::from_xdg_session_type(Some("Wayland")),
            SessionKind::Wayland
        );
    }

    #[test]
    fn everything_else_is_other() {
        for value in [Some("tty"), Some("mir"), Some(""), None] {
            assert_eq!(
                SessionKind::from_xdg_session_type(value),
                SessionKind::Other
            );
        }
    }

    #[test]
    fn selects_window_backend_from_session_and_layer_shell_support() {
        assert_eq!(
            WindowBackend::select(SessionKind::X11, false, false),
            WindowBackend::X11
        );
        assert_eq!(
            WindowBackend::select(SessionKind::X11, true, true),
            WindowBackend::X11
        );
        assert_eq!(
            WindowBackend::select(SessionKind::Wayland, true, true),
            WindowBackend::WaylandLayerShell
        );
        assert_eq!(
            WindowBackend::select(SessionKind::Wayland, false, true),
            WindowBackend::WaylandFallback
        );
        assert_eq!(
            WindowBackend::select(SessionKind::Wayland, true, false),
            WindowBackend::WaylandFallback
        );
        assert_eq!(
            WindowBackend::select(SessionKind::Other, true, true),
            WindowBackend::Other
        );
    }

    #[test]
    fn rect_serde_roundtrip_uses_short_field_names() {
        let rect = Rect {
            x: -3,
            y: 7,
            w: 120,
            h: 48,
        };
        let json = serde_json::to_string(&rect).unwrap();
        assert_eq!(json, r#"{"x":-3,"y":7,"w":120,"h":48}"#);
        let back: Rect = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rect);
    }

    #[test]
    fn rect_rejects_negative_size() {
        let result = serde_json::from_str::<Rect>(r#"{"x":0,"y":0,"w":-1,"h":5}"#);
        assert!(result.is_err());
    }
}

#[cfg(all(test, unix))]
mod signal_tests {
    use super::*;

    #[test]
    fn signalling_a_group_reaches_a_grandchild() {
        let mut command = std::process::Command::new("sh");
        // The child outlives its own exit through a background grandchild.
        command.arg("-c").arg("sleep 30 & sleep 30");
        assert!(detach_into_own_process_group(&mut command));
        let mut child = command.spawn().expect("spawn");
        let pgid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(
            signal_process_group(pgid, ProcessSignal::Kill),
            "killpg must reach the group"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = child.wait();
        // Nothing may be left in the group.
        assert!(
            !signal_process_group(pgid, ProcessSignal::Kill),
            "the group should be empty now"
        );
    }
}
