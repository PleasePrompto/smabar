//! The WinRT side of the Windows media adapter: one `Worker` per thread owns
//! the GlobalSystemMediaTransportControls session manager and reads sessions
//! into the raw values `gsmtc` maps. Every async WinRT call is waited for with
//! a deadline; nothing here may block longer than `METHOD_TIMEOUT` per call.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use windows::ApplicationModel::AppInfo;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
};
use windows::core::{HSTRING, RuntimeType};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

use crate::providers::{MediaAction, MediaData, MediaSession};

use super::common::{MAX_SESSIONS, METHOD_TIMEOUT, media_data};
use super::gsmtc::{self, RawControls, RawSession, RawTimeline};
use super::windows::{MediaError, backend};

const FIRST_RETRY: Duration = Duration::from_millis(100);
const MAX_RETRY: Duration = Duration::from_secs(2);
const MAX_CACHED_NAMES: usize = 64;

/// A session as enumerated for one sample or action.
struct Listed {
    id: String,
    aumid: String,
    session: Session,
}

struct Backoff {
    delay: Duration,
    next_attempt: Option<Instant>,
    last_error: Option<MediaError>,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            delay: FIRST_RETRY,
            next_attempt: None,
            last_error: None,
        }
    }
}

#[derive(Default)]
pub(super) struct Worker {
    manager: Option<SessionManager>,
    backoff: Backoff,
    /// AUMID → display name; misses are cached too, so an unpackaged player
    /// costs one failed lookup instead of one per second.
    names: HashMap<String, Option<String>>,
}

impl Worker {
    pub(super) fn sample(&mut self) -> Result<MediaData, MediaError> {
        let manager = self.manager()?;
        let result = self.read_all(&manager);
        if result.is_err() {
            // Enumeration failing means the manager itself is stale; the next
            // request asks Windows for a fresh one.
            self.manager = None;
        }
        result
    }

    pub(super) fn action(
        &mut self,
        session_id: Option<&str>,
        action: MediaAction,
    ) -> Result<(), MediaError> {
        let manager = self.manager()?;
        let listed = match enumerate(&manager) {
            Ok(listed) => listed,
            Err(error) => {
                self.manager = None;
                return Err(error);
            }
        };
        let target = match session_id {
            Some(requested) => listed
                .iter()
                .find(|entry| entry.id == requested)
                .ok_or_else(|| MediaError::SessionGone(requested.to_string()))?,
            None => {
                let current = self
                    .read_all(&manager)?
                    .current_session_id
                    .ok_or(MediaError::NoSession)?;
                listed
                    .iter()
                    .find(|entry| entry.id == current)
                    .ok_or(MediaError::SessionGone(current))?
            }
        };
        let (operation, name) = match action {
            MediaAction::Play => (target.session.TryPlayAsync(), "play"),
            MediaAction::Pause => (target.session.TryPauseAsync(), "pause"),
            MediaAction::PlayPause => (target.session.TryTogglePlayPauseAsync(), "playPause"),
            MediaAction::Next => (target.session.TrySkipNextAsync(), "next"),
            MediaAction::Previous => (target.session.TrySkipPreviousAsync(), "previous"),
        };
        let operation =
            operation.map_err(|error| backend("failed to send the media action", error))?;
        if await_operation(operation, name)? {
            Ok(())
        } else {
            Err(MediaError::Declined(name))
        }
    }

    fn manager(&mut self) -> Result<SessionManager, MediaError> {
        if let Some(manager) = &self.manager {
            return Ok(manager.clone());
        }
        if let (Some(next_attempt), Some(error)) =
            (self.backoff.next_attempt, &self.backoff.last_error)
            && Instant::now() < next_attempt
        {
            return Err(error.clone());
        }
        match request_manager() {
            Ok(manager) => {
                self.manager = Some(manager.clone());
                self.backoff = Backoff::default();
                Ok(manager)
            }
            Err(error) => {
                self.backoff.next_attempt = Some(Instant::now() + self.backoff.delay);
                self.backoff.delay = (self.backoff.delay * 2).min(MAX_RETRY);
                self.backoff.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn read_all(&mut self, manager: &SessionManager) -> Result<MediaData, MediaError> {
        let listed = enumerate(manager)?;
        let current = current_id(manager, &listed);
        let now = gsmtc::now_100ns(SystemTime::now());
        let deadline = Instant::now() + METHOD_TIMEOUT;
        let mut sessions = Vec::with_capacity(listed.len());
        for (index, entry) in listed.iter().enumerate() {
            if Instant::now() >= deadline {
                tracing::debug!(
                    skipped = listed.len() - index,
                    "media sample budget exhausted; remaining sessions wait for the next sample"
                );
                break;
            }
            match self.read_session(entry, now) {
                Ok(Some(session)) => sessions.push(session),
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(session = %entry.id, %error, "skipping unreadable media session");
                }
            }
        }
        let mut data = media_data(sessions);
        if let Some(current) = current
            && data.sessions.iter().any(|session| session.id == current)
        {
            data.current_session_id = Some(current);
        }
        Ok(data)
    }

    fn read_session(
        &mut self,
        entry: &Listed,
        now: i64,
    ) -> Result<Option<MediaSession>, MediaError> {
        let info = entry
            .session
            .GetPlaybackInfo()
            .map_err(|error| backend("failed to read media playback info", error))?;
        let status = info
            .PlaybackStatus()
            .map_err(|error| backend("failed to read media playback status", error))?
            .0;
        if gsmtc::playback_state(status).is_none() {
            return Ok(None);
        }
        let controls = info
            .Controls()
            .map_err(|error| backend("failed to read media playback controls", error))?;
        let flag = |value: windows::core::Result<bool>| {
            value.map_err(|error| backend("failed to read a media control flag", error))
        };
        let controls = RawControls {
            play: flag(controls.IsPlayEnabled())?,
            pause: flag(controls.IsPauseEnabled())?,
            toggle: flag(controls.IsPlayPauseToggleEnabled())?,
            next: flag(controls.IsNextEnabled())?,
            previous: flag(controls.IsPreviousEnabled())?,
        };
        let timeline = entry
            .session
            .GetTimelineProperties()
            .map_err(|error| backend("failed to read the media timeline", error))?;
        let span = |value: windows::core::Result<windows::Foundation::TimeSpan>| {
            value
                .map(|span| span.Duration)
                .map_err(|error| backend("failed to read a media timeline value", error))
        };
        let timeline = RawTimeline {
            start: span(timeline.StartTime())?,
            end: span(timeline.EndTime())?,
            position: span(timeline.Position())?,
            last_updated: timeline
                .LastUpdatedTime()
                .map(|time| time.UniversalTime)
                .map_err(|error| backend("failed to read the media timeline age", error))?,
        };
        let properties = entry
            .session
            .TryGetMediaPropertiesAsync()
            .map_err(|error| backend("failed to request media properties", error))
            .and_then(|operation| await_operation(operation, "media properties"))?;
        let field = |value: windows::core::Result<HSTRING>| {
            value
                .map(|text| text.to_string())
                .map_err(|error| backend("failed to read a media property", error))
        };
        let raw = RawSession {
            aumid: entry.aumid.clone(),
            display_name: self.display_name(&entry.aumid),
            status,
            title: field(properties.Title())?,
            artist: field(properties.Artist())?,
            album_artist: field(properties.AlbumArtist())?,
            album: field(properties.AlbumTitle())?,
            controls,
            timeline,
        };
        Ok(gsmtc::session(entry.id.clone(), raw, now))
    }

    fn display_name(&mut self, aumid: &str) -> Option<String> {
        if let Some(cached) = self.names.get(aumid) {
            return cached.clone();
        }
        if self.names.len() >= MAX_CACHED_NAMES {
            self.names.clear();
        }
        // Unpackaged players (`Spotify.exe`) have no AppInfo and Windows 10
        // before 2004 has no lookup at all; both fall back to the AUMID.
        let name = AppInfo::GetFromAppUserModelId(&HSTRING::from(aumid))
            .and_then(|info| info.DisplayInfo())
            .and_then(|display| display.DisplayName())
            .ok()
            .map(|name| name.to_string())
            .filter(|name| !name.trim().is_empty());
        self.names.insert(aumid.to_string(), name.clone());
        name
    }
}

fn request_manager() -> Result<SessionManager, MediaError> {
    SessionManager::RequestAsync()
        .map_err(|error| backend("failed to request the Windows media session manager", error))
        .and_then(|operation| await_operation(operation, "session manager"))
}

fn enumerate(manager: &SessionManager) -> Result<Vec<Listed>, MediaError> {
    let sessions = manager
        .GetSessions()
        .map_err(|error| backend("failed to list Windows media sessions", error))?;
    let count = sessions
        .Size()
        .map_err(|error| backend("failed to count Windows media sessions", error))?;
    let mut handles = Vec::new();
    for index in 0..count {
        if handles.len() >= MAX_SESSIONS {
            break;
        }
        let session = sessions
            .GetAt(index)
            .map_err(|error| backend("failed to read a Windows media session", error))?;
        handles.push(session);
    }
    let aumids: Vec<String> = handles
        .iter()
        .map(|session| {
            session
                .SourceAppUserModelId()
                .map(|id| id.to_string())
                .unwrap_or_default()
        })
        .collect();
    let ids = gsmtc::session_ids(aumids.iter().map(String::as_str));
    Ok(handles
        .into_iter()
        .zip(aumids)
        .zip(ids)
        .map(|((session, aumid), id)| Listed { id, aumid, session })
        .collect())
}

/// The session Windows itself would control (the volume flyout's choice);
/// `None` when there is none, which is not a failure.
fn current_id(manager: &SessionManager, listed: &[Listed]) -> Option<String> {
    let current = manager.GetCurrentSession().ok()?;
    let aumid = current.SourceAppUserModelId().ok()?.to_string();
    listed
        .iter()
        .find(|entry| entry.aumid == aumid)
        .map(|entry| entry.id.clone())
}

/// Waits at most `METHOD_TIMEOUT` for a WinRT operation. `join()` would wait
/// forever on a player that never answers.
fn await_operation<T: RuntimeType + 'static>(
    operation: IAsyncOperation<T>,
    what: &'static str,
) -> Result<T, MediaError> {
    let (done, finished) = mpsc::channel::<()>();
    operation
        .SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
            let _ = done.send(());
            Ok(())
        }))
        .map_err(|error| backend("failed to observe a Windows media operation", error))?;
    let status = |operation: &IAsyncOperation<T>| {
        operation
            .Status()
            .map_err(|error| backend("failed to read a Windows media operation status", error))
    };
    if finished.recv_timeout(METHOD_TIMEOUT).is_err()
        && status(&operation)?.0 == AsyncStatus::Started.0
    {
        let _ = operation.Cancel();
        return Err(MediaError::Timeout);
    }
    let status = status(&operation)?;
    if status.0 == AsyncStatus::Completed.0 {
        return operation.GetResults().map_err(|error| backend(what, error));
    }
    let code = operation
        .ErrorCode()
        .map(|code| format!("{code:?}"))
        .unwrap_or_default();
    Err(MediaError::Backend(format!(
        "Windows media {what} operation ended with status {} {code}",
        status.0
    )))
}
