//! Local-file playback on one native worker; the UI and RPC reader never decode.
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufReader, Seek as _};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use anyhow::Context;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use serde::Deserialize;
use serde_json::{Value, json};
use smabar_core::config::ConfigWatcher;
use smabar_core::plugins::HostSession;
use tokio::sync::oneshot;

use crate::desktop_types::{AudioPlay, valid_id};

const PER_PLUGIN_LIMIT: usize = 4;
const TOTAL_LIMIT: usize = 32;
const HISTORY_LIMIT: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(100);
type Key = (u64, String);

struct Request {
    owner: HostSession,
    method: String,
    params: Value,
    notification: bool,
    reply: oneshot::Sender<Result<Value, String>>,
}

#[derive(Clone)]
pub struct AudioService {
    tx: mpsc::SyncSender<Request>,
}

impl AudioService {
    pub fn start(config: Arc<ConfigWatcher>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(64);
        let runtime = tokio::runtime::Handle::current();
        std::thread::Builder::new().name("smabar-audio".into()).spawn(move || {
            let mut worker = Worker {
                output: None, failed: Arc::new(AtomicBool::new(false)),
                tracks: HashMap::new(), history: VecDeque::new(), config, runtime,
            };
            loop {
                match rx.recv_timeout(POLL_INTERVAL) {
                    Ok(request) => {
                        let result = worker.handle(&request).map_err(|error| {
                            tracing::warn!(plugin = %request.owner.plugin_id, method = %request.method, %error, "audio request failed; check source and output device");
                            format!("{error:#}")
                        });
                        let _ = request.reply.send(result);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                worker.refresh();
            }
        }).context("failed to start native audio worker")?;
        Ok(Self { tx })
    }

    pub async fn request(
        &self,
        owner: HostSession,
        method: &str,
        params: Value,
        notification: bool,
    ) -> Result<Value, String> {
        let (reply, receiver) = oneshot::channel();
        self.tx
            .try_send(Request {
                owner,
                method: method.into(),
                params,
                notification,
                reply,
            })
            .map_err(|_| "audio worker is busy or unavailable; retry later".to_string())?;
        tokio::time::timeout(Duration::from_secs(5), receiver)
            .await
            .map_err(|_| {
                "audio worker timed out; query playback status before retrying".to_string()
            })?
            .map_err(|_| "audio worker stopped".to_string())?
    }
}

struct Track {
    owner: HostSession,
    id: String,
    player: Player,
    duration_ms: Option<u64>,
    volume: u8,
    looping: bool,
    notification: bool,
}

impl Track {
    fn status(&self, state: &str) -> Value {
        json!({"playbackId": self.id, "state": state,
            "positionMs": self.player.get_pos().as_millis().min(u128::from(u64::MAX)) as u64,
            "durationMs": self.duration_ms, "volume": self.volume, "loop": self.looping})
    }
}

struct Worker {
    output: Option<MixerDeviceSink>,
    failed: Arc<AtomicBool>,
    tracks: HashMap<Key, Track>,
    history: VecDeque<(Key, Value)>,
    config: Arc<ConfigWatcher>,
    runtime: tokio::runtime::Handle,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Control {
    playback_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Volume {
    playback_id: String,
    volume: u8,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Seek {
    playback_id: String,
    position_ms: u64,
}

impl Worker {
    fn handle(&mut self, request: &Request) -> anyhow::Result<Value> {
        if request.owner.stopped.is_cancelled() || request.reply.is_closed() {
            anyhow::bail!("audio request belongs to an ended session or expired call");
        }
        if request.method == "audio.play" {
            return self.play(request);
        }
        let mut volume = None;
        let mut position = None;
        let id = match request.method.as_str() {
            "audio.setVolume" => {
                let value: Volume = serde_json::from_value(request.params.clone())?;
                if value.volume > 100 {
                    anyhow::bail!("volume must be an integer from 0 to 100");
                }
                volume = Some(value.volume);
                value.playback_id
            }
            "audio.seek" => {
                let value: Seek = serde_json::from_value(request.params.clone())?;
                position = Some(value.position_ms);
                value.playback_id
            }
            "audio.pause" | "audio.resume" | "audio.stop" | "audio.status" => {
                serde_json::from_value::<Control>(request.params.clone())?.playback_id
            }
            _ => anyhow::bail!(
                "unknown audio method; use play, pause, resume, stop, setVolume, seek or status"
            ),
        };
        if !valid_id(&id) {
            anyhow::bail!("invalid playbackId; use 1–128 letters, digits, '.', '_' or '-'");
        }
        let key = (request.owner.generation, id.clone());
        if request.method == "audio.stop" {
            self.finish(&key, "stopped", None);
            return Ok(json!({"playbackId": id, "state": "stopped"}));
        }
        let Some(track) = self.tracks.get_mut(&key) else {
            if request.method == "audio.status"
                && let Some((_, status)) = self.history.iter().find(|(k, _)| *k == key)
            {
                return Ok(status.clone());
            }
            anyhow::bail!("unknown playbackId for this plugin session; start playback first");
        };
        match request.method.as_str() {
            "audio.pause" => track.player.pause(),
            "audio.resume" => track.player.play(),
            _ => {}
        }
        if let Some(volume) = volume {
            track.volume = volume;
            track.player.set_volume(self.config.current().audio.gain(
                &track.owner.plugin_id,
                volume,
                track.notification,
            ));
        }
        if let Some(position) = position {
            track
                .player
                .try_seek(Duration::from_millis(position))
                .context("this source could not seek to the requested position")?;
        }
        let status = track.status(if track.player.is_paused() {
            "paused"
        } else {
            "playing"
        });
        let owner = track.owner.clone();
        if request.method != "audio.status" {
            self.event(owner, status.clone());
        }
        Ok(status)
    }

    fn play(&mut self, request: &Request) -> anyhow::Result<Value> {
        let args: AudioPlay = serde_json::from_value(request.params.clone())?;
        if !valid_id(&args.playback_id) || args.volume > 100 {
            anyhow::bail!(
                "playbackId must use 1–128 letters, digits, '.', '_' or '-'; volume must be 0–100"
            );
        }
        let key = (request.owner.generation, args.playback_id.clone());
        if !self.tracks.contains_key(&key)
            && (self.tracks.len() >= TOTAL_LIMIT
                || self
                    .tracks
                    .values()
                    .filter(|t| t.owner.plugin_id == request.owner.plugin_id)
                    .count()
                    >= PER_PLUGIN_LIMIT)
        {
            anyhow::bail!(
                "audio limit reached (4 per plugin, 32 total); stop another playback first"
            );
        }
        let mut file = File::open(
            args.source
                .resolve(&request.owner.plugin_dir, &request.owner.data_dir)?,
        )
        .context("audio file could not be opened")?;
        let decoder = Decoder::try_from(file.try_clone()?)
            .context("unsupported or corrupt audio file; use WAV, MP3, Ogg/Vorbis or FLAC")?;
        let duration_ms = decoder
            .total_duration()
            .and_then(|d| u64::try_from(d.as_millis()).ok());
        if self.output.is_none() {
            let failed = self.failed.clone();
            let output = DeviceSinkBuilder::from_default_device()
                .context("no default audio output is available; connect an output device and retry")?
                .with_error_callback(move |error| {
                    tracing::error!(%error, "audio output failed; reconnect the device and start playback again");
                    failed.store(true, Ordering::Release);
                }).open_stream().context("could not open default audio output; check system audio settings")?;
            self.output = Some(output);
        }
        let output = self.output.as_ref().context("audio output unavailable")?;
        let player = Player::connect_new(output.mixer());
        player.pause();
        player.set_volume(self.config.current().audio.gain(
            &request.owner.plugin_id,
            args.volume,
            request.notification,
        ));
        if args.looping {
            // Loop the seekable decoder, not repeat_infinite(), which caches all samples.
            drop(decoder);
            file.rewind()?;
            player.append(looped_decoder(file)?);
        } else {
            player.append(decoder);
        }
        if request.owner.stopped.is_cancelled() || request.reply.is_closed() {
            anyhow::bail!("audio call expired before playback began");
        }
        self.finish(&key, "stopped", None);
        self.history.retain(|(k, _)| *k != key);
        player.play();
        let track = Track {
            owner: request.owner.clone(),
            id: args.playback_id,
            player,
            duration_ms,
            volume: args.volume,
            looping: args.looping,
            notification: request.notification,
        };
        let status = track.status("playing");
        self.event(track.owner.clone(), status.clone());
        self.tracks.insert(key, track);
        Ok(status)
    }

    fn refresh(&mut self) {
        let failed = self.failed.swap(false, Ordering::AcqRel);
        let mut finished = Vec::new();
        let config = self.config.current();
        for (key, track) in &self.tracks {
            if track.owner.stopped.is_cancelled() || failed || track.player.empty() {
                finished.push((
                    key.clone(),
                    if failed {
                        "error"
                    } else if track.owner.stopped.is_cancelled() {
                        "stopped"
                    } else {
                        "ended"
                    },
                ));
            } else {
                track.player.set_volume(config.audio.gain(
                    &track.owner.plugin_id,
                    track.volume,
                    track.notification,
                ));
            }
        }
        for (key, state) in finished {
            self.finish(
                &key,
                state,
                failed.then_some("output device failed; start playback again after reconnecting"),
            );
        }
        if self.tracks.is_empty() || failed {
            self.output = None;
        }
    }

    fn finish(&mut self, key: &Key, state: &str, error: Option<&str>) {
        if let Some(track) = self.tracks.remove(key) {
            let mut status = track.status(state);
            if let Some(error) = error {
                status["error"] = json!(error);
            }
            track.player.stop();
            self.event(track.owner, status.clone());
            self.history.retain(|(k, _)| k != key);
            self.history.push_back((key.clone(), status));
            while self.history.len() > HISTORY_LIMIT {
                self.history.pop_front();
            }
        }
    }

    fn event(&self, owner: HostSession, status: Value) {
        self.runtime.spawn(async move {
            owner.notify("audio.event", status).await;
        });
    }
}

fn looped_decoder(file: File) -> anyhow::Result<rodio::decoder::LoopedDecoder<BufReader<File>>> {
    let len = file.metadata()?.len();
    Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(len)
        .build_looped()
        .context("could not loop audio source")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looped_file_remains_seekable_after_wrapping() {
        let mut file = tempfile::tempfile().expect("file");
        std::io::Write::write_all(
            &mut file,
            include_bytes!("../../../../plugins/todos/sounds/reminder.wav"),
        )
        .expect("write tone");
        file.rewind().expect("rewind");
        let mut source = looped_decoder(file).expect("decode");
        for _ in 0..u32::from(source.sample_rate()) * 2 {
            assert!(source.next().is_some(), "loop must not end");
        }
        source
            .try_seek(Duration::from_millis(100))
            .expect("seek loop");
        assert!(source.next().is_some());
    }
}
