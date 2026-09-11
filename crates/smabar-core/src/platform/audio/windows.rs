use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, STGM_READ,
};

use crate::providers::{AudioAction, AudioData, AudioOutput};

use super::{AudioError, Command, backend_error, set_snapshot};

pub(super) const SUPPORTED: bool = true;

const REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const MAX_RETRY: Duration = Duration::from_secs(2);
type SharedSnapshot = Arc<Mutex<Result<AudioData, AudioError>>>;

pub(super) fn spawn(
    snapshot: SharedSnapshot,
    commands: mpsc::Receiver<Command>,
) -> Result<(), AudioError> {
    thread::Builder::new()
        .name("smabar-audio".to_string())
        .spawn(move || worker(snapshot, commands))
        .map(|_| ())
        .map_err(|error| backend_error("failed to start Windows audio worker", error))
}

fn worker(snapshot: SharedSnapshot, commands: mpsc::Receiver<Command>) {
    let mut retry = Duration::from_millis(100);
    loop {
        let initialized = ComGuard::initialize()
            .and_then(|com| AudioBackend::new().map(|backend| (com, backend)));
        match initialized {
            Ok((_com, backend)) => {
                set_snapshot(&snapshot, backend.sample());
                loop {
                    match commands.recv_timeout(REFRESH_INTERVAL) {
                        Ok(command) => {
                            let result = if command.expired() {
                                Err(AudioError::ActionTimeout)
                            } else {
                                backend.apply_action(command.action)
                            };
                            let _ = command.reply.send(result);
                            set_snapshot(&snapshot, backend.sample());
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            set_snapshot(&snapshot, backend.sample());
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
            Err(error) => {
                set_snapshot(&snapshot, Err(error.clone()));
                match commands.recv_timeout(retry) {
                    Ok(command) => {
                        let result = if command.expired() {
                            Err(AudioError::ActionTimeout)
                        } else {
                            Err(error)
                        };
                        let _ = command.reply.send(result);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                retry = (retry * 2).min(MAX_RETRY);
            }
        }
    }
}

struct ComGuard;

impl ComGuard {
    fn initialize() -> Result<Self, AudioError> {
        // SAFETY: this dedicated worker owns the matching CoUninitialize in
        // Drop and no COM object leaves the thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| backend_error("failed to initialize Windows COM", error))?;
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        // SAFETY: paired with the successful CoInitializeEx on this thread.
        unsafe { CoUninitialize() };
    }
}

struct AudioBackend {
    enumerator: IMMDeviceEnumerator,
}

impl AudioBackend {
    fn new() -> Result<Self, AudioError> {
        // SAFETY: COM is initialized on this worker and the returned interface
        // remains on it until before CoUninitialize.
        let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|error| backend_error("failed to create Windows audio enumerator", error))?;
        Ok(Self { enumerator })
    }

    fn sample(&self) -> Result<AudioData, AudioError> {
        let Some(device) = self.default_device()? else {
            return Ok(AudioData::default());
        };
        let endpoint = endpoint_volume(&device)?;
        // SAFETY: endpoint is a live interface on this initialized COM thread.
        let volume = unsafe { endpoint.GetMasterVolumeLevelScalar() }
            .map_err(|error| backend_error("failed to read Windows output volume", error))?;
        // SAFETY: same as above.
        let muted = unsafe { endpoint.GetMute() }
            .map_err(|error| backend_error("failed to read Windows output mute", error))?
            .as_bool();
        Ok(AudioData {
            default_output: Some(AudioOutput {
                name: device_name(&device),
                volume_percent: (volume * 100.0).clamp(0.0, 100.0),
                muted,
            }),
        })
    }

    fn apply_action(&self, action: AudioAction) -> Result<(), AudioError> {
        let device = self
            .default_device()?
            .ok_or_else(|| AudioError::Backend("no default audio output is available".into()))?;
        let endpoint = endpoint_volume(&device)?;
        match action {
            AudioAction::SetVolume(percent) => {
                // SAFETY: the validated u8 maps to Core Audio's required
                // normalized 0.0..=1.0 range; null means no callback tag.
                unsafe {
                    endpoint
                        .SetMasterVolumeLevelScalar(f32::from(percent) / 100.0, std::ptr::null())
                }
                .map_err(|error| backend_error("failed to set Windows output volume", error))
            }
            AudioAction::SetMuted(muted) => set_muted(&endpoint, muted),
        }
    }

    fn default_device(&self) -> Result<Option<IMMDevice>, AudioError> {
        // eConsole is the system/interactions endpoint used by Microsoft's
        // taskbar-style endpoint-volume sample, not a media-session role.
        match unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(device) => Ok(Some(device)),
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
            Err(error) => Err(backend_error(
                "failed to read the default Windows audio output",
                error,
            )),
        }
    }
}

fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume, AudioError> {
    // SAFETY: device is a live endpoint on an initialized COM thread.
    unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|error| backend_error("failed to open Windows endpoint volume", error))
}

fn set_muted(endpoint: &IAudioEndpointVolume, muted: bool) -> Result<(), AudioError> {
    // SAFETY: endpoint is a live interface on an initialized COM thread;
    // null means the notification has no caller-specific event GUID.
    unsafe { endpoint.SetMute(muted, std::ptr::null()) }
        .map_err(|error| backend_error("failed to set Windows output mute", error))
}

fn device_name(device: &IMMDevice) -> String {
    // A missing friendly name must not make volume control unavailable.
    let name = unsafe {
        device
            .OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
    };
    match name {
        Ok(value) => {
            let value = value.to_string();
            if value.trim().is_empty() {
                String::new()
            } else {
                value.chars().take(256).collect()
            }
        }
        // A friendly name is presentation metadata, not a prerequisite for
        // controlling the endpoint. Keep the stable fallback noise-free.
        Err(_) => String::new(),
    }
}
