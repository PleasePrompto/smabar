//! The OS owns login registration. smabar only remembers that its default-on
//! attempt happened, so later starts never undo an external opt-out.

use std::sync::{Arc, Mutex};

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use smabar_core::config::ConfigWatcher;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_autostart::AutoLaunchManager;

#[cfg(target_os = "linux")]
#[path = "autostart_linux.rs"]
mod linux;
#[cfg(windows)]
#[path = "autostart_windows.rs"]
mod windows;

const CHANGED: &str = "autostart-changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AutostartStatus {
    Unavailable,
    Ready { registered: bool },
    Failed { registered: Option<bool> },
}

impl AutostartStatus {
    pub fn registered(&self) -> Option<bool> {
        match self {
            Self::Ready { registered } => Some(*registered),
            Self::Failed { registered } => *registered,
            Self::Unavailable => None,
        }
    }
}

struct Autostart {
    watcher: Arc<ConfigWatcher>,
    // Serializes Settings and tray operations, including their read-back.
    registration: Mutex<Registration>,
}

struct Registration {
    status: AutostartStatus,
    initialization_failed: bool,
}

enum Action {
    Read,
    Set(bool),
    Toggle,
}

/// Called after single-instance and config loading, never in --provision.
pub fn setup(app: &AppHandle, watcher: Arc<ConfigWatcher>) {
    let (status, initialization_failed) = if cfg!(debug_assertions) {
        (AutostartStatus::Unavailable, false)
    } else {
        let result = initialize(&watcher, true, || {
            apply_registration(
                true,
                || registered(app),
                |enabled| write_registration(app, enabled),
            )
        });
        let failed = result.is_err();
        if let Err(error) = result {
            log_failure("initialize", &error);
        }
        (read_status(app, failed), failed)
    };
    app.manage(Autostart {
        watcher,
        registration: Mutex::new(Registration {
            status,
            initialization_failed,
        }),
    });
}

/// Claim before enabling: a crash or failed registration must not create an
/// endless default-on retry that can undo a subsequent external opt-out.
fn initialize(
    watcher: &ConfigWatcher,
    release: bool,
    enable: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<()> {
    if release && mark_initialized(watcher)? {
        ensure!(enable()?, "autostart registration was not confirmed");
    }
    Ok(())
}

fn mark_initialized(watcher: &ConfigWatcher) -> anyhow::Result<bool> {
    watcher
        .update(|current| {
            let mut new = current.clone();
            new.autostart_initialized = true;
            (new, !current.autostart_initialized)
        })
        .context("could not save the autostart initialization marker")
}

#[cfg(not(target_os = "linux"))]
fn manager(app: &AppHandle) -> anyhow::Result<tauri::State<'_, AutoLaunchManager>> {
    if app.try_state::<AutoLaunchManager>().is_none() {
        // Register lazily so plugin setup failure cannot abort the bar startup
        // and an explicit user retry can try again. LaunchAgent is the default.
        app.plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("smabar")
                .build(),
        )
        .context("could not initialize desktop login registration")?;
    }
    app.try_state().context("autostart manager is unavailable")
}

#[cfg(target_os = "linux")]
fn entry_file(app: &AppHandle) -> anyhow::Result<std::path::PathBuf> {
    Ok(smabar_core::config::SmabarPaths::desktop_autostart_file(
        &app.path().config_dir()?,
    ))
}

fn registered(app: &AppHandle) -> anyhow::Result<bool> {
    #[cfg(windows)]
    if windows::packaged()? {
        return windows::registered();
    }
    #[cfg(target_os = "linux")]
    return linux::registered(&entry_file(app)?);
    #[cfg(not(target_os = "linux"))]
    Ok(manager(app)?.is_enabled()?)
}

fn write_registration(app: &AppHandle, enabled: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    if windows::packaged()? {
        return windows::set(enabled);
    }
    #[cfg(target_os = "linux")]
    return linux::set(&entry_file(app)?, &std::env::current_exe()?, enabled);
    #[cfg(not(target_os = "linux"))]
    {
        let manager = manager(app)?;
        if enabled {
            #[cfg(windows)]
            {
                let executable = std::env::current_exe()?;
                let result = manager
                    .enable()
                    .map_err(anyhow::Error::from)
                    .and_then(|()| windows::quote_run_entry(&executable));
                if let Err(error) = result {
                    if let Err(cleanup) = manager.disable() {
                        tracing::error!(%cleanup, "could not remove the incomplete autostart entry; disable smabar in Windows Startup settings");
                    }
                    return Err(error);
                }
            }
            #[cfg(not(windows))]
            manager.enable()?;
        } else {
            manager.disable()?;
        }
        Ok(())
    }
}

fn apply_registration(
    enabled: bool,
    mut read: impl FnMut() -> anyhow::Result<bool>,
    write: impl FnOnce(bool) -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    if read()? != enabled {
        write(enabled)?;
    }
    let registered = read()?;
    ensure!(
        registered == enabled,
        "autostart change was not confirmed by the operating system"
    );
    Ok(registered)
}

fn read_status(app: &AppHandle, failed: bool) -> AutostartStatus {
    match registered(app) {
        Ok(registered) if !failed => AutostartStatus::Ready { registered },
        Ok(registered) => AutostartStatus::Failed {
            registered: Some(registered),
        },
        Err(error) => {
            log_failure("read", &error);
            AutostartStatus::Failed { registered: None }
        }
    }
}

fn log_failure(operation: &str, error: &anyhow::Error) {
    tracing::error!(operation, error = ?error,
        "autostart operation failed; retry in Settings > System or check your operating system's login settings");
}

async fn run(app: AppHandle, action: Action) -> Result<AutostartStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Autostart>();
        let mut previous = state.registration.lock().map_err(|error| {
            tracing::error!(%error, "autostart state is poisoned; restart smabar");
            "autostart unavailable; restart smabar".to_string()
        })?;
        if cfg!(debug_assertions) {
            return Ok(AutostartStatus::Unavailable);
        }
        let result = (|| -> anyhow::Result<Option<bool>> {
            let requested = match action {
                Action::Read => return Ok(None),
                Action::Set(enabled) => enabled,
                Action::Toggle => !registered(&app)?,
            };
                mark_initialized(&state.watcher)?;
                apply_registration(requested, || registered(&app), |enabled| {
                    write_registration(&app, enabled)
                }).map(Some)
            })();
        let failed = match result {
                Ok(None) => previous.initialization_failed,
                Ok(Some(registered)) => {
                    previous.initialization_failed = false;
                    tracing::info!(registered, "autostart registration confirmed");
                    false
                }
                Err(error) => {
                    log_failure("set", &error);
                    true
                }
        };
        let status = read_status(&app, failed);
        if (status != previous.status || !matches!(action, Action::Read))
            && let Err(error) = app.emit(CHANGED, &status)
        {
                tracing::warn!(%error, "could not refresh autostart controls; reopen Settings > System");
        }
        previous.status = status.clone();
        Ok(status)
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "autostart worker stopped; retry in Settings > System");
        "autostart worker stopped; retry in Settings > System".to_string()
    })?
}

#[tauri::command]
pub async fn get_autostart_status(app: AppHandle) -> Result<AutostartStatus, String> {
    run(app, Action::Read).await
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<AutostartStatus, String> {
    run(app, Action::Set(enabled)).await
}

pub async fn toggle(app: AppHandle) -> Result<AutostartStatus, String> {
    run(app, Action::Toggle).await
}

/// Tray construction only needs the snapshot already read during setup.
pub fn current(app: &AppHandle) -> anyhow::Result<AutostartStatus> {
    app.state::<Autostart>()
        .registration
        .lock()
        .map(|registration| registration.status.clone())
        .map_err(|_| anyhow::anyhow!("autostart state is poisoned; restart smabar"))
}

#[cfg(test)]
#[path = "autostart_tests.rs"]
mod tests;
