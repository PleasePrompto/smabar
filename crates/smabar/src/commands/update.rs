//! Application updates (ADR 0009). `check_update` asks the endpoint for a
//! newer release; `install_update` downloads it signature-verified and then
//! lets the updater plugin apply it (Windows runs the installer and the
//! process exits; macOS swaps the bundle and the app restarts) or hands the
//! package to the system installer (Linux).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterBuilder, UpdaterExt};

use super::AppState;
use crate::platform::{self, InstallMode};

/// Progress reaches the shell at most once per this many bytes.
const PROGRESS_STEP: u64 = 256 * 1024;

/// A newer release as the settings panel presents it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    /// The server's `pub_date` verbatim; the shell formats it for the locale.
    pub date: Option<String>,
    /// How this build installs — `null` shows the release without a button.
    pub installer: Option<InstallMode>,
}

/// `install_update`'s answer where the system installer takes over.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandOff {
    pub path: PathBuf,
    /// False when the package is saved but the installer refused to open.
    pub opened: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    received: u64,
    total: Option<u64>,
    /// The bytes are complete and verified; what follows is the install.
    finished: bool,
}

/// Target and endpoint are decided here for check and install alike.
fn build_updater(app: &AppHandle) -> Result<UpdaterBuilder, String> {
    let mut builder = app.updater_builder();
    if let Some(target) = platform::updater_target() {
        builder = builder.target(target);
    }
    // The app edge reads the environment: dev.sh points this at the local
    // store, a release build may point at staging. Safe because signature
    // verification cannot be disabled — and release builds still refuse
    // plain http here.
    if let Some(endpoint) = std::env::var("SMABAR_UPDATE_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        let url: tauri::Url = endpoint
            .parse()
            .map_err(|error| format!("SMABAR_UPDATE_ENDPOINT is not a URL: {error}"))?;
        builder = builder
            .endpoints(vec![url])
            .map_err(|error| error.to_string())?;
    }
    Ok(builder)
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = build_updater(app)?
        .build()
        .map_err(|error| error.to_string())?;
    updater.check().await.map_err(|error| {
        tracing::warn!(
            %error,
            "update check failed; the bar keeps running — check the connection or SMABAR_UPDATE_ENDPOINT"
        );
        error.to_string()
    })
}

/// Asks the update endpoint for a newer release. `None` means the installed
/// version is current. An error is a failed CHECK (offline, wrong endpoint)
/// — package signatures are only verified when a download happens.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = find_update(&app).await?;
    if update.is_some() {
        // Notifications are lazy; an app update must also work before any
        // plugin has opened a popup. The new window requests the latest state.
        app.state::<crate::surfaces::SurfaceManager>()
            .prepare_notifications(&app)
            .map_err(|error| {
                tracing::warn!(%error, "failed to prepare the application update notification");
                format!(
                    "Cannot show the update notification: {error}; check again or restart smabar"
                )
            })?;
    }
    Ok(update.map(|update| {
        tracing::info!(version = %update.version, "a newer smabar release is available");
        UpdateInfo {
            version: update.version,
            notes: update.body,
            date: update
                .raw_json
                .get("pub_date")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            installer: platform::install_mode(),
        }
    }))
}

/// Downloads and applies exactly the release the user saw. On Windows and
/// macOS this never returns `Ok`: Windows ends the process once the installer
/// runs, macOS restarts into the new bundle. One install at a time — the
/// shell's disabled button is not a lock against a second IPC call or a
/// future MCP tool.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    expected_version: String,
) -> Result<HandOff, String> {
    if state.update_in_progress.swap(true, Ordering::AcqRel) {
        return Err("an update is already running".to_string());
    }
    let result = run_install(&app, &state, &expected_version).await;
    state.update_in_progress.store(false, Ordering::Release);
    result
}

async fn run_install(
    app: &AppHandle,
    state: &AppState,
    expected_version: &str,
) -> Result<HandOff, String> {
    let mode = platform::install_mode()
        .ok_or_else(|| "updates are not supported on this platform".to_string())?;
    let update = find_update(app)
        .await?
        .filter(|update| update.version == expected_version)
        .ok_or_else(|| {
            format!("the release changed since the last check (expected {expected_version}); check again")
        })?;
    let bytes = download(app, &update).await?;
    match mode {
        InstallMode::App => {
            if cfg!(target_os = "macos") {
                refuse_unmovable_bundle()?;
            } else {
                // Windows: the plugin runs the installer and ends the process
                // with exit(0), so RunEvent::Exit never fires — release what
                // that handler would, plugin children and then the native
                // reservation, before handing over.
                state.shutdown_plugins().await;
                shutdown_platform_on_main_thread(app).await?;
            }
            update.install(bytes).map_err(|error| {
                tracing::error!(%error, "the update could not be installed");
                error.to_string()
            })?;
            if cfg!(target_os = "macos") {
                // The plugin swapped the bundle in place and returned. The
                // restart goes through RunEvent::Exit, which stops plugins and
                // releases the reservation before the new version starts.
                app.restart();
            }
            Err("the installer did not take over; restart smabar".to_string())
        }
        InstallMode::System => {
            let dir = state.paths().updates_dir();
            clear_dir(&dir)?;
            let last_segment = update
                .download_url
                .path_segments()
                .and_then(|mut segments| segments.next_back());
            let file = dir.join(package_file_name(
                last_segment,
                &update.target,
                &update.version,
            ));
            std::fs::write(&file, &bytes)
                .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
            let opened = match state.shortcuts.open_path(&file) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        path = %file.display(),
                        "the system installer did not open; install the saved package manually"
                    );
                    false
                }
            };
            tracing::info!(path = %file.display(), opened, "update package handed to the system installer");
            Ok(HandOff { path: file, opened })
        }
    }
}

/// Verified bytes of the release; the plugin checks the minisign signature
/// before returning them, so a tampered package never reaches the disk.
async fn download(app: &AppHandle, update: &Update) -> Result<Vec<u8>, String> {
    let mut received = 0u64;
    let mut total = None;
    let mut gate = ProgressGate::default();
    let bytes = update
        .download(
            |chunk, length| {
                received += chunk as u64;
                total = length;
                if gate.should_emit(received) {
                    emit_progress(app, received, total, false);
                }
            },
            || {},
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, "update download or signature check failed; nothing was changed");
            error.to_string()
        })?;
    emit_progress(app, received, total, true);
    Ok(bytes)
}

fn emit_progress(app: &AppHandle, received: u64, total: Option<u64>, finished: bool) {
    let _ = app.emit(
        "update-progress",
        Progress {
            received,
            total,
            finished,
        },
    );
}

/// Lets the first chunk through, then one event per `PROGRESS_STEP`.
#[derive(Default)]
struct ProgressGate {
    last: u64,
}

impl ProgressGate {
    fn should_emit(&mut self, received: u64) -> bool {
        if self.last == 0 || received.saturating_sub(self.last) >= PROGRESS_STEP {
            self.last = received;
            return true;
        }
        false
    }
}

/// A bundle running from the mounted DMG or from Gatekeeper's translocation
/// copy cannot be swapped in place; it has to be installed first.
fn refuse_unmovable_bundle() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let path = exe.to_string_lossy();
    if path.starts_with("/Volumes/") || path.contains("/AppTranslocation/") {
        return Err("move smabar to the Applications folder before updating".to_string());
    }
    Ok(())
}

/// The AppBar proxy window belongs to the main thread; releasing it from a
/// worker fails, so the exit handler's platform step runs there.
async fn shutdown_platform_on_main_thread(app: &AppHandle) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(platform::shutdown().map_err(|error| error.to_string()));
    })
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "the main thread dropped the shutdown result".to_string())?
}

/// One package at a time: stale downloads are removed before the new one lands.
fn clear_dir(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot clear {}: {error}", dir.display())),
    }
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))
}

/// The cache file name for a package: the server's own when it is plain and
/// carries the target's extension, otherwise a generated one — a hostile
/// manifest must not be able to name a path outside `updates_dir`.
fn package_file_name(segment: Option<&str>, target: &str, version: &str) -> String {
    let format = if target.contains("-rpm-") {
        "rpm"
    } else {
        "deb"
    };
    let extension = format!(".{format}");
    segment
        .filter(|name| {
            name.ends_with(&extension)
                && name.len() > extension.len()
                && !name.starts_with('.')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._+~-".contains(c))
        })
        .map_or_else(|| format!("smabar-{version}.{format}"), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{PROGRESS_STEP, ProgressGate, package_file_name};

    #[test]
    fn package_file_name_keeps_plain_names_and_replaces_the_rest() {
        let deb = "linux-deb-x86_64";
        assert_eq!(
            package_file_name(Some("smabar_0.2.0_amd64.deb"), deb, "0.2.0"),
            "smabar_0.2.0_amd64.deb"
        );
        assert_eq!(package_file_name(None, deb, "0.2.0"), "smabar-0.2.0.deb");
        assert_eq!(
            package_file_name(Some(".."), deb, "0.2.0"),
            "smabar-0.2.0.deb"
        );
        assert_eq!(
            package_file_name(Some("smabar.exe"), deb, "0.2.0"),
            "smabar-0.2.0.deb"
        );
        assert_eq!(
            package_file_name(Some("sm abar.deb"), deb, "0.2.0"),
            "smabar-0.2.0.deb"
        );
        assert_eq!(
            package_file_name(Some("x.deb"), "linux-rpm-x86_64", "0.2.0"),
            "smabar-0.2.0.rpm"
        );
    }

    #[test]
    fn progress_gate_lets_the_first_chunk_and_every_step_through() {
        let mut gate = ProgressGate::default();
        assert!(gate.should_emit(1));
        assert!(!gate.should_emit(PROGRESS_STEP));
        assert!(gate.should_emit(PROGRESS_STEP + 1));
        assert!(!gate.should_emit(PROGRESS_STEP + 2));
    }
}
