//! Bounds and selection rules every native media adapter shares.

#[cfg(any(target_os = "linux", windows))]
use std::time::Duration;

use crate::providers::{MediaData, MediaPlaybackState, MediaSession};

/// Sessions beyond this many are ignored; no desktop runs this many players.
#[cfg(any(target_os = "linux", windows))]
pub(super) const MAX_SESSIONS: usize = 32;
pub(super) const MAX_TEXT_CHARS: usize = 512;
pub(super) const MAX_ARTIST_CHARS: usize = 256;
/// Players are local IPC peers; one that cannot answer within this window
/// must not stall provider sampling or plugin actions indefinitely.
#[cfg(any(target_os = "linux", windows))]
pub(super) const METHOD_TIMEOUT: Duration = Duration::from_secs(3);

/// Sorts sessions by id and picks the current one: the first playing, else
/// the first paused, else the first stopped session.
pub(super) fn media_data(mut sessions: Vec<MediaSession>) -> MediaData {
    sessions.sort_by(|a, b| a.id.cmp(&b.id));
    let current_session_id = [
        MediaPlaybackState::Playing,
        MediaPlaybackState::Paused,
        MediaPlaybackState::Stopped,
    ]
    .into_iter()
    .find_map(|state| {
        sessions
            .iter()
            .find(|session| session.playback_state == state)
            .map(|session| session.id.clone())
    });
    MediaData {
        current_session_id,
        sessions,
    }
}

pub(super) fn bounded(mut value: String, max_chars: usize) -> String {
    if let Some((byte_index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(byte_index);
    }
    value
}

#[cfg(test)]
pub(super) fn session_fixture(id: &str, playback_state: MediaPlaybackState) -> MediaSession {
    MediaSession {
        id: id.to_string(),
        identity: "Player".to_string(),
        desktop_entry: None,
        playback_state,
        title: None,
        artists: Vec::new(),
        album: None,
        art_url: None,
        position_ms: None,
        duration_ms: None,
        can_control: false,
        can_play: false,
        can_pause: false,
        can_go_next: false,
        can_go_previous: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_bounded_on_character_boundaries() {
        assert_eq!(bounded("éclair".to_string(), 2), "éc");
        assert_eq!(bounded("ok".to_string(), 8), "ok");
    }

    #[test]
    fn current_session_is_selected_deterministically() {
        let data = media_data(vec![
            session_fixture("zeta", MediaPlaybackState::Playing),
            session_fixture("alpha", MediaPlaybackState::Paused),
            session_fixture("beta", MediaPlaybackState::Playing),
        ]);

        assert_eq!(data.current_session_id.as_deref(), Some("beta"));
        assert_eq!(
            data.sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "zeta"]
        );
    }

    #[test]
    fn paused_and_stopped_sessions_fall_back_in_order() {
        let paused = media_data(vec![
            session_fixture("b", MediaPlaybackState::Stopped),
            session_fixture("a", MediaPlaybackState::Paused),
        ]);
        assert_eq!(paused.current_session_id.as_deref(), Some("a"));

        let stopped = media_data(vec![session_fixture("only", MediaPlaybackState::Stopped)]);
        assert_eq!(stopped.current_session_id.as_deref(), Some("only"));
        assert_eq!(media_data(Vec::new()).current_session_id, None);
    }
}
