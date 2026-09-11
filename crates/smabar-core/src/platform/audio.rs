//! Native system-output volume access. The provider contract is neutral;
//! only these adapters know PulseAudio/PipeWire-Pulse or Windows Core Audio.

use std::sync::{Arc, Mutex, mpsc};

use crate::util::lock_unpoisoned;
use std::time::{Duration, Instant};

use crate::providers::{AudioAction, AudioData};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
use windows as imp;

const ACTION_TIMEOUT: Duration = Duration::from_secs(3);
const ACTION_REPLY_GRACE: Duration = Duration::from_millis(100);

pub(crate) const AUDIO_SUPPORTED: bool = imp::SUPPORTED;

#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum AudioError {
    #[error("{0}")]
    Backend(String),
    #[error("the audio provider worker is unavailable")]
    WorkerUnavailable,
    #[error("the audio provider operation timed out")]
    ActionTimeout,
}

type Snapshot = Arc<Mutex<Result<AudioData, AudioError>>>;

pub(super) struct Command {
    pub action: AudioAction,
    pub deadline: Instant,
    pub reply: mpsc::Sender<Result<(), AudioError>>,
}

impl Command {
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

/// One native worker per provider hub. Linux keeps its `!Send` PulseAudio
/// mainloop on that thread; Windows keeps COM initialized on that thread.
#[derive(Clone)]
pub(crate) struct AudioSource {
    snapshot: Snapshot,
    commands: mpsc::Sender<Command>,
}

impl Default for AudioSource {
    fn default() -> Self {
        // Startup is a normal no-device snapshot until the worker publishes
        // its first native sample; backend failures replace it with an error.
        let snapshot = Arc::new(Mutex::new(Ok(AudioData::default())));
        let (commands, receiver) = mpsc::channel();
        if let Err(error) = imp::spawn(Arc::clone(&snapshot), receiver) {
            set_snapshot(&snapshot, Err(error));
        }
        Self { snapshot, commands }
    }
}

impl AudioSource {
    pub(crate) fn sample(&self) -> Result<AudioData, AudioError> {
        lock_unpoisoned(&self.snapshot).clone()
    }

    pub(crate) fn action(&self, action: AudioAction) -> Result<(), AudioError> {
        let (reply, response) = mpsc::channel();
        let deadline = Instant::now() + ACTION_TIMEOUT;
        self.commands
            .send(Command {
                action,
                deadline,
                reply,
            })
            .map_err(|_| AudioError::WorkerUnavailable)?;
        response
            .recv_timeout(ACTION_TIMEOUT + ACTION_REPLY_GRACE)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => AudioError::ActionTimeout,
                mpsc::RecvTimeoutError::Disconnected => AudioError::WorkerUnavailable,
            })?
    }
}

pub(super) fn set_snapshot(snapshot: &Snapshot, value: Result<AudioData, AudioError>) {
    *lock_unpoisoned(snapshot) = value;
}

pub(super) fn backend_error(context: &str, error: impl std::fmt::Display) -> AudioError {
    AudioError::Backend(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_commands_are_detectable_before_the_worker_mutates_audio() {
        let (reply, _response) = mpsc::channel();
        let command = Command {
            action: AudioAction::SetMuted(true),
            deadline: Instant::now(),
            reply,
        };
        assert!(command.expired());
    }
}
