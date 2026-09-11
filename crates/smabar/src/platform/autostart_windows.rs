//! auto-launch 0.5 writes a raw executable path to Run. Quote the value
//! after its registration, retaining its per-user StartupApproved handling.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use anyhow::{Context, ensure};
use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows::core::w;

pub(super) fn packaged() -> anyhow::Result<bool> {
    let mut length = 0;
    // SAFETY: the documented size-only query writes only to the live length.
    match unsafe { GetCurrentPackageFullName(&mut length, None) } {
        APPMODEL_ERROR_NO_PACKAGE => Ok(false),
        ERROR_INSUFFICIENT_BUFFER => Ok(true),
        error => Err(anyhow::anyhow!(
            "could not inspect package identity: {error:?}"
        )),
    }
}

fn task() -> anyhow::Result<StartupTask> {
    StartupTask::GetAsync(&"smabar".into())?
        .join()
        .context("could not read the packaged smabar startup task")
}

fn enabled(state: StartupTaskState) -> bool {
    state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
}

pub(super) fn registered() -> anyhow::Result<bool> {
    Ok(enabled(task()?.State()?))
}

pub(super) fn set(enable: bool) -> anyhow::Result<()> {
    let task = task()?;
    if enable {
        let state = task.RequestEnableAsync()?.join()?;
        ensure!(
            enabled(state),
            "Windows disabled smabar startup; enable it in Windows Settings > Apps > Startup"
        );
    } else {
        task.Disable()?;
    }
    Ok(())
}

pub(super) fn quote_run_entry(executable: &Path) -> anyhow::Result<()> {
    let command = startup_command(executable);
    let bytes = u32::try_from(std::mem::size_of_val(command.as_slice()))?;
    // SAFETY: predefined per-user root; the literal key/value and command
    // are NUL-terminated UTF-16 and remain live for this synchronous call.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"),
            w!("smabar"),
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes,
        )
        .ok()
    }
    .context("could not quote the Windows autostart executable")
}

fn startup_command(executable: &Path) -> Vec<u16> {
    std::iter::once(u16::from(b'"'))
        .chain(executable.as_os_str().encode_wide())
        .chain([u16::from(b'"'), 0])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_path_is_quoted_utf16_with_a_terminator() {
        let actual = startup_command(Path::new(r"C:\Users\Zoë Smith\smabar\smabar.exe"));
        let expected: Vec<u16> = "\"C:\\Users\\Zoë Smith\\smabar\\smabar.exe\"\0"
            .encode_utf16()
            .collect();
        assert_eq!(actual, expected);
    }
}
