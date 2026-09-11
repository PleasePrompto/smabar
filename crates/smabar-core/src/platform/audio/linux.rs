use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::context::introspect::SinkInfo;
use pulse::context::subscribe::InterestMaskSet;
use pulse::context::{Context, FlagSet, State as ContextState};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::operation::{Operation, State as OperationState};
use pulse::proplist::{Proplist, properties};
use pulse::volume::{ChannelVolumes, Volume};

use crate::providers::{AudioAction, AudioData, AudioOutput};

use super::{AudioError, Command, backend_error, set_snapshot};

pub(super) const SUPPORTED: bool = true;

const OPERATION_TIMEOUT: Duration = Duration::from_secs(2);
const LOOP_PAUSE: Duration = Duration::from_millis(10);
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
        .map_err(|error| backend_error("failed to start Linux audio worker", error))
}

fn worker(snapshot: SharedSnapshot, commands: mpsc::Receiver<Command>) {
    let mut retry = Duration::from_millis(100);
    loop {
        match PulseBackend::connect() {
            Ok(mut backend) => {
                retry = Duration::from_millis(100);
                set_snapshot(&snapshot, backend.read_data());
                match backend.run(&snapshot, &commands) {
                    WorkerExit::Stopped => return,
                    WorkerExit::Reconnect(error) => set_snapshot(&snapshot, Err(error)),
                }
            }
            Err(error) => set_snapshot(&snapshot, Err(error.clone())),
        }

        match commands.recv_timeout(retry) {
            Ok(command) => {
                let error = if command.expired() {
                    AudioError::ActionTimeout
                } else {
                    AudioError::Backend("Linux audio service is unavailable".to_string())
                };
                let _ = command.reply.send(Err(error));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                retry = (retry * 2).min(MAX_RETRY);
            }
        }
    }
}

enum WorkerExit {
    Stopped,
    Reconnect(AudioError),
}

struct PulseBackend {
    mainloop: Mainloop,
    context: Context,
    dirty: Arc<AtomicBool>,
}

impl PulseBackend {
    fn connect() -> Result<Self, AudioError> {
        let mainloop = Mainloop::new()
            .ok_or_else(|| AudioError::Backend("failed to create PulseAudio mainloop".into()))?;
        let mut properties = Proplist::new()
            .ok_or_else(|| AudioError::Backend("failed to create PulseAudio properties".into()))?;
        properties
            .set_str(properties::APPLICATION_NAME, "smabar")
            .map_err(|()| AudioError::Backend("failed to name PulseAudio client".into()))?;
        properties
            .set_str(properties::APPLICATION_ID, "smabar")
            .map_err(|()| AudioError::Backend("failed to identify PulseAudio client".into()))?;
        let mut context = Context::new_with_proplist(&mainloop, "smabar", &properties)
            .ok_or_else(|| AudioError::Backend("failed to create PulseAudio context".into()))?;
        context
            .connect(None, FlagSet::NOAUTOSPAWN, None)
            .map_err(|error| backend_error("failed to connect to PulseAudio", error))?;

        let mut backend = Self {
            mainloop,
            context,
            dirty: Arc::new(AtomicBool::new(true)),
        };
        backend.wait_until_ready()?;
        backend.subscribe()?;
        Ok(backend)
    }

    fn run(&mut self, snapshot: &SharedSnapshot, commands: &mpsc::Receiver<Command>) -> WorkerExit {
        loop {
            loop {
                match commands.try_recv() {
                    Ok(command) => {
                        let result = if command.expired() {
                            Err(AudioError::ActionTimeout)
                        } else {
                            self.apply_action(command.action, command.deadline)
                        };
                        let succeeded = result.is_ok();
                        let _ = command.reply.send(result);
                        if succeeded {
                            set_snapshot(snapshot, self.read_data());
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return WorkerExit::Stopped,
                }
            }
            if self.dirty.swap(false, Ordering::AcqRel) {
                set_snapshot(snapshot, self.read_data());
            }
            if let Err(error) = self.pump() {
                return WorkerExit::Reconnect(error);
            }
            thread::sleep(LOOP_PAUSE);
        }
    }

    fn wait_until_ready(&mut self) -> Result<(), AudioError> {
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        loop {
            match self.context.get_state() {
                ContextState::Ready => return Ok(()),
                ContextState::Failed | ContextState::Terminated => {
                    return Err(backend_error(
                        "PulseAudio connection failed",
                        self.context.errno(),
                    ));
                }
                _ if Instant::now() >= deadline => {
                    return Err(AudioError::Backend(
                        "PulseAudio connection timed out".to_string(),
                    ));
                }
                _ => {
                    self.pump()?;
                    thread::sleep(LOOP_PAUSE);
                }
            }
        }
    }

    fn subscribe(&mut self) -> Result<(), AudioError> {
        let dirty = Arc::clone(&self.dirty);
        self.context
            .set_subscribe_callback(Some(Box::new(move |_, _, _| {
                dirty.store(true, Ordering::Release);
            })));
        let success = Rc::new(Cell::new(None));
        let result = Rc::clone(&success);
        let mut operation =
            self.context
                .subscribe(InterestMaskSet::SINK | InterestMaskSet::SERVER, move |ok| {
                    result.set(Some(ok));
                });
        self.wait_operation(&mut operation, Instant::now() + OPERATION_TIMEOUT)?;
        if success.get() == Some(true) {
            Ok(())
        } else {
            Err(AudioError::Backend(
                "PulseAudio event subscription failed".to_string(),
            ))
        }
    }

    fn read_data(&mut self) -> Result<AudioData, AudioError> {
        let default_output = self
            .default_sink(Instant::now() + OPERATION_TIMEOUT)?
            .map(|sink| AudioOutput {
                name: bounded_name(&sink.display_name),
                volume_percent: volume_percent(sink.volume.max()),
                muted: sink.muted,
            });
        Ok(AudioData { default_output })
    }

    fn apply_action(&mut self, action: AudioAction, deadline: Instant) -> Result<(), AudioError> {
        let sink = self
            .default_sink(deadline)?
            .ok_or_else(|| AudioError::Backend("no default audio output is available".into()))?;
        match action {
            AudioAction::SetVolume(percent) => {
                let mut volumes = sink.volume;
                let target = percent_volume(percent);
                if volumes.max() == Volume::MUTED {
                    volumes.set(volumes.len(), target);
                } else if volumes.scale(target).is_none() {
                    return Err(AudioError::Backend(
                        "PulseAudio rejected the output channel volumes".into(),
                    ));
                }
                let success = Rc::new(Cell::new(None));
                let result = Rc::clone(&success);
                let mut operation = self.context.introspect().set_sink_volume_by_name(
                    &sink.internal_name,
                    &volumes,
                    Some(Box::new(move |ok| result.set(Some(ok)))),
                );
                self.wait_success(
                    &mut operation,
                    &success,
                    "setting PulseAudio volume",
                    deadline,
                )
            }
            AudioAction::SetMuted(muted) => self.set_muted(&sink.internal_name, muted, deadline),
        }
    }

    fn set_muted(&mut self, sink: &str, muted: bool, deadline: Instant) -> Result<(), AudioError> {
        let success = Rc::new(Cell::new(None));
        let result = Rc::clone(&success);
        let mut operation = self.context.introspect().set_sink_mute_by_name(
            sink,
            muted,
            Some(Box::new(move |ok| result.set(Some(ok)))),
        );
        self.wait_success(
            &mut operation,
            &success,
            "setting PulseAudio mute",
            deadline,
        )
    }

    fn default_sink(&mut self, deadline: Instant) -> Result<Option<Sink>, AudioError> {
        let default_name = Rc::new(RefCell::new(None));
        let result = Rc::clone(&default_name);
        let mut operation = self.context.introspect().get_server_info(move |info| {
            *result.borrow_mut() =
                Some(info.default_sink_name.as_ref().map(|name| name.to_string()));
        });
        self.wait_operation(&mut operation, deadline)?;
        let default_name = default_name.borrow_mut().take().ok_or_else(|| {
            AudioError::Backend("PulseAudio returned no server information".into())
        })?;
        let Some(default_name) = default_name else {
            return Ok(None);
        };

        let sink = Rc::new(RefCell::new(None));
        let failed = Rc::new(Cell::new(false));
        let sink_result = Rc::clone(&sink);
        let failed_result = Rc::clone(&failed);
        let mut operation = self.context.introspect().get_sink_info_by_name(
            &default_name,
            move |item| match item {
                ListResult::Item(info) => *sink_result.borrow_mut() = Some(Sink::from(info)),
                ListResult::Error => failed_result.set(true),
                ListResult::End => {}
            },
        );
        self.wait_operation(&mut operation, deadline)?;
        if failed.get() {
            return Err(AudioError::Backend(
                "PulseAudio failed to read the default output".into(),
            ));
        }
        Ok(sink.borrow_mut().take())
    }

    fn wait_success<C: ?Sized>(
        &mut self,
        operation: &mut Operation<C>,
        success: &Cell<Option<bool>>,
        context: &str,
        deadline: Instant,
    ) -> Result<(), AudioError> {
        self.wait_operation(operation, deadline)?;
        if success.get() == Some(true) {
            Ok(())
        } else {
            Err(AudioError::Backend(format!("{context} failed")))
        }
    }

    fn wait_operation<C: ?Sized>(
        &mut self,
        operation: &mut Operation<C>,
        deadline: Instant,
    ) -> Result<(), AudioError> {
        while operation.get_state() == OperationState::Running {
            if Instant::now() >= deadline {
                operation.cancel();
                return Err(AudioError::ActionTimeout);
            }
            self.pump()?;
            thread::sleep(LOOP_PAUSE);
        }
        if operation.get_state() == OperationState::Done {
            Ok(())
        } else {
            Err(AudioError::Backend(
                "PulseAudio operation was cancelled".to_string(),
            ))
        }
    }

    fn pump(&mut self) -> Result<(), AudioError> {
        match self.mainloop.iterate(false) {
            IterateResult::Success(_) => {}
            IterateResult::Quit(_) => {
                return Err(AudioError::Backend("PulseAudio mainloop stopped".into()));
            }
            IterateResult::Err(error) => {
                return Err(backend_error("PulseAudio mainloop failed", error));
            }
        }
        match self.context.get_state() {
            ContextState::Failed | ContextState::Terminated => Err(backend_error(
                "PulseAudio connection closed",
                self.context.errno(),
            )),
            _ => Ok(()),
        }
    }
}

impl Drop for PulseBackend {
    fn drop(&mut self) {
        self.context.disconnect();
    }
}

struct Sink {
    internal_name: String,
    display_name: String,
    volume: ChannelVolumes,
    muted: bool,
}

impl From<&SinkInfo<'_>> for Sink {
    fn from(info: &SinkInfo<'_>) -> Self {
        let internal_name = info
            .name
            .as_ref()
            .map_or_else(String::new, ToString::to_string);
        let display_name = info
            .description
            .as_ref()
            .map_or_else(|| internal_name.clone(), ToString::to_string);
        Self {
            internal_name,
            display_name,
            volume: info.volume,
            muted: info.mute,
        }
    }
}

fn bounded_name(value: &str) -> String {
    value.chars().take(256).collect()
}

fn volume_percent(volume: Volume) -> f32 {
    (volume.0 as f32 / Volume::NORMAL.0 as f32 * 100.0).clamp(0.0, 100.0)
}

fn percent_volume(percent: u8) -> Volume {
    let raw = u64::from(Volume::NORMAL.0) * u64::from(percent) / 100;
    Volume(u32::try_from(raw).unwrap_or(Volume::NORMAL.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_conversion_is_bounded_and_round_trips() {
        assert_eq!(percent_volume(0), Volume::MUTED);
        assert_eq!(percent_volume(100), Volume::NORMAL);
        assert_eq!(volume_percent(Volume(Volume::NORMAL.0 * 2)), 100.0);
        assert!((volume_percent(percent_volume(37)) - 37.0).abs() < 0.01);
    }

    #[test]
    fn scaling_preserves_channel_balance() {
        let mut channels = ChannelVolumes::default();
        channels.set(2, Volume::NORMAL);
        channels.get_mut()[1] = Volume(Volume::NORMAL.0 / 2);
        let target = percent_volume(40);
        assert!(channels.scale(target).is_some());
        assert_eq!(channels.max(), target);
        assert_eq!(channels.get()[1].0, target.0 / 2);
    }
}
