//! Pure mapping from GlobalSystemMediaTransportControls values to the neutral
//! media contract. Nothing here touches WinRT, so these tests run on every
//! host; the adapter in `windows.rs` only reads raw values and calls in.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::providers::{MediaPlaybackState, MediaSession};

use super::common::{MAX_ARTIST_CHARS, MAX_TEXT_CHARS, bounded};

// `GlobalSystemMediaTransportControlsSessionPlaybackStatus`; WinRT enums are
// stable ABI values, and matching on them keeps this file free of bindings.
pub(super) const STATUS_CLOSED: i32 = 0;
pub(super) const STATUS_OPENED: i32 = 1;
pub(super) const STATUS_CHANGING: i32 = 2;
pub(super) const STATUS_STOPPED: i32 = 3;
pub(super) const STATUS_PLAYING: i32 = 4;
pub(super) const STATUS_PAUSED: i32 = 5;

const HUNDRED_NS_PER_MS: i64 = 10_000;
/// Distance between the Windows epoch (1601-01-01) and the Unix epoch in
/// 100-nanosecond units, the resolution of WinRT `DateTime`.
const WINDOWS_EPOCH_OFFSET: i64 = 116_444_736_000_000_000;

/// One session as read on the worker thread; everything else is derived here.
pub(super) struct RawSession {
    pub aumid: String,
    pub display_name: Option<String>,
    pub status: i32,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub controls: RawControls,
    pub timeline: RawTimeline,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RawControls {
    pub play: bool,
    pub pause: bool,
    pub toggle: bool,
    pub next: bool,
    pub previous: bool,
}

/// Timeline values in 100-nanosecond units as `TimeSpan` / `DateTime` carry
/// them; `last_updated` is a Windows-epoch timestamp.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RawTimeline {
    pub start: i64,
    pub end: i64,
    pub position: i64,
    pub last_updated: i64,
}

/// `None` for a closed session: the player is gone and only the manager has
/// not dropped it yet.
pub(super) fn playback_state(status: i32) -> Option<MediaPlaybackState> {
    match status {
        STATUS_PLAYING => Some(MediaPlaybackState::Playing),
        STATUS_PAUSED => Some(MediaPlaybackState::Paused),
        STATUS_OPENED | STATUS_CHANGING | STATUS_STOPPED => Some(MediaPlaybackState::Stopped),
        STATUS_CLOSED => None,
        _ => None,
    }
}

/// Current time as a WinRT `DateTime` value.
pub(super) fn now_100ns(now: SystemTime) -> i64 {
    now.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since_unix| i64::try_from(since_unix.as_nanos() / 100).ok())
        .map_or(0, |unix| unix.saturating_add(WINDOWS_EPOCH_OFFSET))
}

/// Session ids: the AppUserModelId, suffixed `#2`, `#3`, … when one app owns
/// several sessions at once.
// ponytail: the suffix follows enumeration order, so two tabs of one browser
// can swap ids between samples; a stable per-session key needs the GSMTC
// event model.
pub(super) fn session_ids<'a>(aumids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    aumids
        .into_iter()
        .map(|aumid| {
            let count = seen.entry(aumid).or_insert(0);
            *count += 1;
            if *count == 1 {
                aumid.to_string()
            } else {
                format!("{aumid}#{count}")
            }
        })
        .collect()
}

/// Player name: the app's display name when Windows resolved the AUMID,
/// otherwise the AUMID without its package prefix or `.exe` suffix.
pub(super) fn identity(aumid: &str, display_name: Option<&str>) -> String {
    if let Some(name) = display_name.map(str::trim).filter(|name| !name.is_empty()) {
        return bounded(name.to_string(), MAX_TEXT_CHARS);
    }
    let name = aumid.rsplit_once('!').map_or(aumid, |(_, app)| app);
    let name = if name.to_ascii_lowercase().ends_with(".exe") {
        &name[..name.len() - ".exe".len()]
    } else {
        name
    };
    let name = if name.trim().is_empty() { aumid } else { name };
    bounded(name.trim().to_string(), MAX_TEXT_CHARS)
}

/// GSMTC exposes one artist string, which is passed through unsplit; the
/// album artist is the fallback so compilations still name someone.
pub(super) fn artists(artist: &str, album_artist: &str) -> Vec<String> {
    [artist, album_artist]
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(|value| vec![bounded(value.to_string(), MAX_ARTIST_CHARS)])
        .unwrap_or_default()
}

/// Position and duration in milliseconds. Players refresh the timeline only
/// every few seconds, so a playing session's position advances by the time
/// elapsed since `last_updated`; a session without a timeline reports neither.
pub(super) fn timeline_ms(
    timeline: RawTimeline,
    playing: bool,
    now: i64,
) -> (Option<u64>, Option<u64>) {
    let total = (timeline.end > timeline.start).then(|| timeline.end - timeline.start);
    if total.is_none() && timeline.position <= timeline.start {
        return (None, None);
    }
    let mut position = timeline.position.saturating_sub(timeline.start);
    if playing && timeline.last_updated > 0 && now > timeline.last_updated {
        position = position.saturating_add(now - timeline.last_updated);
    }
    if let Some(total) = total {
        position = position.min(total);
    }
    (Some(to_ms(position)), total.map(to_ms))
}

fn to_ms(hundred_ns: i64) -> u64 {
    u64::try_from(hundred_ns.max(0) / HUNDRED_NS_PER_MS).unwrap_or(0)
}

fn text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| bounded(value.to_string(), MAX_TEXT_CHARS))
}

/// Neutral session from raw values; `None` for a closed session.
pub(super) fn session(id: String, raw: RawSession, now: i64) -> Option<MediaSession> {
    let playback_state = playback_state(raw.status)?;
    let playing = playback_state == MediaPlaybackState::Playing;
    let (position_ms, duration_ms) = timeline_ms(raw.timeline, playing, now);
    let controls = raw.controls;
    Some(MediaSession {
        id: bounded(id, MAX_TEXT_CHARS),
        identity: identity(&raw.aumid, raw.display_name.as_deref()),
        desktop_entry: None,
        playback_state,
        title: text(&raw.title),
        artists: artists(&raw.artist, &raw.album_artist),
        album: text(&raw.album),
        art_url: None,
        position_ms,
        duration_ms,
        can_control: controls.play
            || controls.pause
            || controls.toggle
            || controls.next
            || controls.previous,
        can_play: controls.play || controls.toggle,
        can_pause: controls.pause || controls.toggle,
        can_go_next: controls.next,
        can_go_previous: controls.previous,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn raw(status: i32) -> RawSession {
        RawSession {
            aumid: "Spotify.exe".to_string(),
            display_name: None,
            status,
            title: " Song ".to_string(),
            artist: String::new(),
            album_artist: "Various".to_string(),
            album: String::new(),
            controls: RawControls {
                toggle: true,
                next: true,
                ..RawControls::default()
            },
            timeline: RawTimeline::default(),
        }
    }

    #[test]
    fn playback_status_maps_every_known_value_and_drops_closed_sessions() {
        assert_eq!(
            playback_state(STATUS_PLAYING),
            Some(MediaPlaybackState::Playing)
        );
        assert_eq!(
            playback_state(STATUS_PAUSED),
            Some(MediaPlaybackState::Paused)
        );
        for stopped in [STATUS_OPENED, STATUS_CHANGING, STATUS_STOPPED] {
            assert_eq!(playback_state(stopped), Some(MediaPlaybackState::Stopped));
        }
        assert_eq!(playback_state(STATUS_CLOSED), None);
        assert_eq!(playback_state(99), None);
    }

    #[test]
    fn session_ids_stay_unique_per_app() {
        let ids = session_ids(["MSEdge", "Spotify.exe", "MSEdge", "MSEdge"]);
        assert_eq!(ids, ["MSEdge", "Spotify.exe", "MSEdge#2", "MSEdge#3"]);
    }

    #[test]
    fn identity_prefers_display_names_and_cleans_aumids() {
        assert_eq!(identity("Spotify.exe", Some(" Spotify ")), "Spotify");
        assert_eq!(identity("Spotify.exe", Some("   ")), "Spotify");
        assert_eq!(identity("vlc.EXE", None), "vlc");
        assert_eq!(
            identity(
                "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
                None
            ),
            "Microsoft.ZuneMusic"
        );
        assert_eq!(identity("MSEdge", None), "MSEdge");
        assert_eq!(identity("weird!", None), "weird!");
    }

    #[test]
    fn artists_fall_back_to_the_album_artist_and_stay_bounded() {
        assert_eq!(artists("A & B", "Various"), ["A & B"]);
        assert_eq!(artists("  ", "Various"), ["Various"]);
        assert!(artists("", " ").is_empty());
        let long = "x".repeat(MAX_ARTIST_CHARS + 5);
        assert_eq!(artists(&long, "")[0].chars().count(), MAX_ARTIST_CHARS);
    }

    #[test]
    fn timelines_extrapolate_only_while_playing_and_clamp_to_the_end() {
        let timeline = RawTimeline {
            start: 0,
            end: 60 * 1_000 * HUNDRED_NS_PER_MS,
            position: 10 * 1_000 * HUNDRED_NS_PER_MS,
            last_updated: 1_000,
        };
        let five_seconds_later = timeline.last_updated + 5 * 1_000 * HUNDRED_NS_PER_MS;

        assert_eq!(
            timeline_ms(timeline, false, five_seconds_later),
            (Some(10_000), Some(60_000))
        );
        assert_eq!(
            timeline_ms(timeline, true, five_seconds_later),
            (Some(15_000), Some(60_000))
        );
        let far_later = timeline.last_updated + 600 * 1_000 * HUNDRED_NS_PER_MS;
        assert_eq!(
            timeline_ms(timeline, true, far_later),
            (Some(60_000), Some(60_000))
        );
    }

    #[test]
    fn missing_timelines_and_live_streams_are_explicit() {
        assert_eq!(timeline_ms(RawTimeline::default(), true, 5), (None, None));
        let live = RawTimeline {
            position: 42 * 1_000 * HUNDRED_NS_PER_MS,
            ..RawTimeline::default()
        };
        assert_eq!(timeline_ms(live, false, 5), (Some(42_000), None));
        let offset = RawTimeline {
            start: 5 * 1_000 * HUNDRED_NS_PER_MS,
            end: 15 * 1_000 * HUNDRED_NS_PER_MS,
            position: 7 * 1_000 * HUNDRED_NS_PER_MS,
            last_updated: 0,
        };
        assert_eq!(timeline_ms(offset, true, 5), (Some(2_000), Some(10_000)));
    }

    #[test]
    fn now_is_expressed_on_the_windows_epoch() {
        assert_eq!(now_100ns(UNIX_EPOCH), WINDOWS_EPOCH_OFFSET);
        assert_eq!(
            now_100ns(UNIX_EPOCH + Duration::from_secs(1)),
            WINDOWS_EPOCH_OFFSET + 10_000_000
        );
    }

    #[test]
    fn sessions_derive_controls_and_drop_closed_players() {
        assert!(session("Spotify.exe".to_string(), raw(STATUS_CLOSED), 0).is_none());

        let session = session("Spotify.exe#2".to_string(), raw(STATUS_PAUSED), 0)
            .expect("paused sessions are reported");
        assert_eq!(session.id, "Spotify.exe#2");
        assert_eq!(session.identity, "Spotify");
        assert_eq!(session.playback_state, MediaPlaybackState::Paused);
        assert_eq!(session.title.as_deref(), Some("Song"));
        assert_eq!(session.artists, ["Various"]);
        assert_eq!(session.album, None);
        assert_eq!(session.position_ms, None);
        assert!(session.can_control && session.can_play && session.can_pause);
        assert!(session.can_go_next && !session.can_go_previous);
        assert_eq!(session.desktop_entry, None);
        assert_eq!(session.art_url, None);
    }
}
