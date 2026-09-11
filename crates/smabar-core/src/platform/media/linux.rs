//! MPRIS 2.2 adapter over the session D-Bus.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use zbus::connection::Builder;
use zbus::fdo::DBusProxy;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

use crate::providers::{MediaAction, MediaData, MediaPlaybackState, MediaSession};

use super::common::{
    MAX_ARTIST_CHARS, MAX_SESSIONS, MAX_TEXT_CHARS, METHOD_TIMEOUT, bounded, media_data,
};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const MAX_ARTISTS: usize = 16;
const MAX_URL_CHARS: usize = 2_048;

pub(crate) const MEDIA_SUPPORTED: bool = true;

/// Operational MPRIS failure. Request-shape validation happens before
/// this layer, so these errors map to a JSON-RPC server error rather than
/// `Invalid params`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MediaError {
    #[error("MPRIS session-bus operation failed: {0}")]
    Bus(#[from] zbus::Error),
    #[error("no MPRIS media session is currently available")]
    NoSession,
    #[error("MPRIS media session {0:?} is no longer running")]
    SessionGone(String),
}

#[derive(Debug, thiserror::Error)]
enum PlayerError {
    #[error(transparent)]
    Bus(#[from] zbus::Error),
    #[error("required property {0} is missing or has the wrong type")]
    InvalidProperty(&'static str),
    #[error("unsupported PlaybackStatus value {0:?}")]
    InvalidPlaybackStatus(String),
}

/// Cached session-bus connection shared by all media sampler configs and
/// actions in one provider hub.
#[derive(Clone, Default)]
pub(crate) struct MediaSource {
    connection: Arc<Mutex<Option<Connection>>>,
}

impl MediaSource {
    pub(crate) async fn sample(&self) -> Result<MediaData, MediaError> {
        let connection = self.connection().await?;
        let names = player_names(&connection).await?;
        read_sessions(&connection, &names).await
    }

    pub(crate) async fn action(
        &self,
        session_id: Option<&str>,
        action: MediaAction,
    ) -> Result<(), MediaError> {
        let connection = self.connection().await?;
        let names = player_names(&connection).await?;
        let target = match session_id {
            Some(requested) => requested_session(&names, requested)?,
            None => read_sessions(&connection, &names)
                .await?
                .current_session_id
                .ok_or(MediaError::NoSession)?,
        };
        let proxy = Proxy::new(&connection, target.as_str(), OBJECT_PATH, PLAYER_INTERFACE).await?;
        let _: () = proxy.call(action_method(action), &()).await?;
        Ok(())
    }

    async fn connection(&self) -> Result<Connection, MediaError> {
        let mut cached = self.connection.lock().await;
        if let Some(connection) = cached.as_ref()
            && !connection.is_closed()
        {
            return Ok(connection.clone());
        }
        let connection = configured_connection(Builder::session()?).build().await?;
        *cached = Some(connection.clone());
        Ok(connection)
    }
}

fn configured_connection(builder: Builder<'_>) -> Builder<'_> {
    builder.method_timeout(METHOD_TIMEOUT)
}

async fn player_names(connection: &Connection) -> Result<Vec<String>, MediaError> {
    let proxy = DBusProxy::new(connection).await?;
    let names = proxy
        .list_names()
        .await
        .map_err(zbus::Error::from)?
        .into_iter()
        .map(|name| name.as_str().to_string())
        .collect();
    Ok(normalize_player_names(names))
}

fn normalize_player_names(mut names: Vec<String>) -> Vec<String> {
    names.retain(|name| name.starts_with(MPRIS_PREFIX));
    names.sort();
    names.truncate(MAX_SESSIONS);
    names
}

fn requested_session(names: &[String], requested: &str) -> Result<String, MediaError> {
    names
        .iter()
        .find(|name| name.as_str() == requested)
        .cloned()
        .ok_or_else(|| MediaError::SessionGone(requested.to_string()))
}

fn action_method(action: MediaAction) -> &'static str {
    match action {
        MediaAction::Play => "Play",
        MediaAction::Pause => "Pause",
        MediaAction::PlayPause => "PlayPause",
        MediaAction::Next => "Next",
        MediaAction::Previous => "Previous",
    }
}

async fn read_sessions(connection: &Connection, names: &[String]) -> Result<MediaData, MediaError> {
    let mut sessions = Vec::with_capacity(names.len());
    for name in names {
        match read_session(connection, name).await {
            Ok(session) => sessions.push(session),
            Err(PlayerError::Bus(error))
                if connection.is_closed() || is_transport_error(&error) =>
            {
                return Err(MediaError::Bus(error));
            }
            Err(error) => {
                tracing::debug!(session = name, %error, "skipping malformed MPRIS player");
            }
        }
    }
    Ok(media_data(sessions))
}

fn is_transport_error(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::InputOutput(_) | zbus::Error::Connection(_, _) | zbus::Error::Handshake(_)
    )
}

async fn read_session(
    connection: &Connection,
    session_id: &str,
) -> Result<MediaSession, PlayerError> {
    let properties = Proxy::new(connection, session_id, OBJECT_PATH, PROPERTIES_INTERFACE).await?;
    let root: HashMap<String, OwnedValue> = properties.call("GetAll", &(ROOT_INTERFACE,)).await?;
    let player: HashMap<String, OwnedValue> =
        properties.call("GetAll", &(PLAYER_INTERFACE,)).await?;

    let identity = required_string(&root, "Identity")?;
    let playback_status = required_string(&player, "PlaybackStatus")?;
    let playback_state = match playback_status.as_str() {
        "Playing" => MediaPlaybackState::Playing,
        "Paused" => MediaPlaybackState::Paused,
        "Stopped" => MediaPlaybackState::Stopped,
        _ => return Err(PlayerError::InvalidPlaybackStatus(playback_status)),
    };
    let metadata = metadata(&player);

    Ok(MediaSession {
        id: bounded(session_id.to_string(), MAX_TEXT_CHARS),
        identity,
        desktop_entry: optional_string(&root, "DesktopEntry", MAX_TEXT_CHARS),
        playback_state,
        title: optional_string(&metadata, "xesam:title", MAX_TEXT_CHARS),
        artists: artists(&metadata),
        album: optional_string(&metadata, "xesam:album", MAX_TEXT_CHARS),
        art_url: optional_string(&metadata, "mpris:artUrl", MAX_URL_CHARS),
        position_ms: microseconds(&player, "Position"),
        duration_ms: microseconds(&metadata, "mpris:length"),
        can_control: optional_bool(&player, "CanControl"),
        can_play: optional_bool(&player, "CanPlay"),
        can_pause: optional_bool(&player, "CanPause"),
        can_go_next: optional_bool(&player, "CanGoNext"),
        can_go_previous: optional_bool(&player, "CanGoPrevious"),
    })
}

fn required_string(
    properties: &HashMap<String, OwnedValue>,
    key: &'static str,
) -> Result<String, PlayerError> {
    optional_string(properties, key, MAX_TEXT_CHARS)
        .filter(|value| !value.is_empty())
        .ok_or(PlayerError::InvalidProperty(key))
}

fn optional_string(
    properties: &HashMap<String, OwnedValue>,
    key: &str,
    max_chars: usize,
) -> Option<String> {
    let value = properties.get(key)?;
    let value = <&str>::try_from(value).ok()?;
    Some(bounded(value.to_string(), max_chars))
}

fn optional_bool(properties: &HashMap<String, OwnedValue>, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn metadata(player_properties: &HashMap<String, OwnedValue>) -> HashMap<String, OwnedValue> {
    player_properties
        .get("Metadata")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| HashMap::try_from(value).ok())
        .unwrap_or_default()
}

fn artists(metadata: &HashMap<String, OwnedValue>) -> Vec<String> {
    let Some(value) = metadata.get("xesam:artist") else {
        return Vec::new();
    };
    let Some(value) = value.try_clone().ok() else {
        return Vec::new();
    };
    let Some(values) = Vec::<String>::try_from(value).ok() else {
        return Vec::new();
    };
    values
        .into_iter()
        .take(MAX_ARTISTS)
        .map(|value| bounded(value, MAX_ARTIST_CHARS))
        .collect()
}

fn microseconds(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<u64> {
    let value = properties.get(key)?;
    let value = i64::try_from(value).ok()?;
    u64::try_from(value).ok().map(|value| value / 1_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, Str};

    #[test]
    fn wrong_metadata_types_are_ignored_without_losing_other_fields() {
        let mut metadata = HashMap::new();
        metadata.insert("xesam:title".to_string(), OwnedValue::from(42_i64));
        metadata.insert("xesam:artist".to_string(), OwnedValue::from(false));
        metadata.insert(
            "xesam:album".to_string(),
            OwnedValue::from(Str::from_static("Album")),
        );
        metadata.insert("mpris:length".to_string(), OwnedValue::from(-1_i64));

        assert_eq!(optional_string(&metadata, "xesam:title", 100), None);
        assert!(artists(&metadata).is_empty());
        assert_eq!(
            optional_string(&metadata, "xesam:album", 100).as_deref(),
            Some("Album")
        );
        assert_eq!(microseconds(&metadata, "mpris:length"), None);
    }

    #[test]
    fn artist_arrays_are_read_as_lists() {
        let array = Array::from(vec!["A", "B"]);
        let mut metadata = HashMap::new();
        metadata.insert(
            "xesam:artist".to_string(),
            OwnedValue::try_from(array).expect("test array is a valid D-Bus value"),
        );
        assert_eq!(artists(&metadata), ["A", "B"]);
    }

    #[test]
    fn discovery_filters_sorts_and_bounds_player_names() {
        let mut names = vec!["org.example.NotAPlayer".to_string()];
        names.extend(
            (0..40)
                .rev()
                .map(|index| format!("{MPRIS_PREFIX}player{index:02}")),
        );
        let names = normalize_player_names(names);

        assert_eq!(names.len(), MAX_SESSIONS);
        assert_eq!(
            names.first().map(String::as_str),
            Some("org.mpris.MediaPlayer2.player00")
        );
        assert_eq!(
            names.last().map(String::as_str),
            Some("org.mpris.MediaPlayer2.player31")
        );
    }

    #[test]
    fn disappeared_sessions_and_action_mapping_are_explicit() {
        let names = vec!["org.mpris.MediaPlayer2.demo".to_string()];
        assert!(matches!(
            requested_session(&names, "org.mpris.MediaPlayer2.gone"),
            Err(MediaError::SessionGone(_))
        ));
        assert_eq!(action_method(MediaAction::Play), "Play");
        assert_eq!(action_method(MediaAction::Pause), "Pause");
        assert_eq!(action_method(MediaAction::PlayPause), "PlayPause");
        assert_eq!(action_method(MediaAction::Next), "Next");
        assert_eq!(action_method(MediaAction::Previous), "Previous");
    }

    #[test]
    fn transport_errors_are_not_misreported_as_broken_players() {
        let io_error = zbus::Error::from(std::io::Error::other("test disconnect"));
        assert!(is_transport_error(&io_error));
        assert!(!is_transport_error(&zbus::Error::InvalidReply));
    }

    #[test]
    fn mpris_connections_bound_unresponsive_method_calls() {
        let builder =
            zbus::connection::Builder::address("unix:path=/tmp/smabar-mpris-timeout-regression")
                .expect("fixed test address is valid");
        let configured = configured_connection(builder);

        assert!(
            format!("{configured:?}").contains("method_timeout: Some(3s)"),
            "every MPRIS proxy call must inherit a finite timeout"
        );
    }
}
