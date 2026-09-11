//! Unsupported hosts have no audio worker, channel, or native command queue.

use crate::providers::{AudioAction, AudioData};

pub(crate) const AUDIO_SUPPORTED: bool = false;

#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum AudioError {
    #[error("the audio provider is not available on this platform")]
    Unavailable,
    #[error("{0}")]
    Backend(String),
}

#[derive(Clone, Default)]
pub(crate) struct AudioSource {
    _private: (),
}

impl AudioSource {
    pub(crate) fn sample(&self) -> Result<AudioData, AudioError> {
        Err(AudioError::Unavailable)
    }

    pub(crate) fn action(&self, _action: AudioAction) -> Result<(), AudioError> {
        Err(AudioError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_audio_reports_an_explicit_error() {
        let source = AudioSource::default();
        assert!(matches!(source.sample(), Err(AudioError::Unavailable)));
        assert!(matches!(
            source.action(AudioAction::SetMuted(true)),
            Err(AudioError::Unavailable)
        ));
    }
}
