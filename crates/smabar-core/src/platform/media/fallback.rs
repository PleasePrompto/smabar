use crate::providers::{MediaAction, MediaData};

pub(crate) const MEDIA_SUPPORTED: bool = false;

#[derive(Debug, thiserror::Error)]
#[error("the media provider is not available on this platform")]
pub(crate) struct MediaError;

#[derive(Clone, Default)]
pub(crate) struct MediaSource {
    _private: (),
}

impl MediaSource {
    pub(crate) async fn sample(&self) -> Result<MediaData, MediaError> {
        Err(MediaError)
    }

    pub(crate) async fn action(
        &self,
        _session_id: Option<&str>,
        _action: MediaAction,
    ) -> Result<(), MediaError> {
        Err(MediaError)
    }
}
