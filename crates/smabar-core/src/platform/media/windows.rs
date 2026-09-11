//! GlobalSystemMediaTransportControls adapter. One worker thread owns the
//! WinRT session manager on an MTA; the async provider API talks to it over
//! channels, so sampling never blocks the runtime and every WinRT wait is
//! bounded. The WinRT calls themselves live in `winrt.rs`.

use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use tokio::sync::oneshot;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use crate::providers::{MediaAction, MediaData};
use crate::util::lock_unpoisoned;

use super::winrt::Worker;

pub(crate) const MEDIA_SUPPORTED: bool = true;

/// Every WinRT wait inside the worker is bounded by `METHOD_TIMEOUT` and a
/// sample spends at most one more of those on its session loop, so this only
/// ever fires for a wedged thread.
const REPLY_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum MediaError {
    #[error("{0}")]
    Backend(String),
    #[error("no media session is currently available")]
    NoSession,
    #[error("media session {0:?} is no longer running")]
    SessionGone(String),
    #[error("the media player declined the {0} action")]
    Declined(&'static str),
    #[error("the media provider worker is unavailable")]
    WorkerUnavailable,
    #[error("the media provider operation timed out")]
    Timeout,
}

enum Request {
    Sample(oneshot::Sender<Result<MediaData, MediaError>>),
    Action {
        session_id: Option<String>,
        action: MediaAction,
        reply: oneshot::Sender<Result<(), MediaError>>,
    },
}

/// The worker starts on first use, so a bar whose plugins never subscribe to
/// media keeps no WinRT thread; it ends when the last source is dropped.
#[derive(Clone, Default)]
pub(crate) struct MediaSource {
    requests: Arc<Mutex<Option<mpsc::Sender<Request>>>>,
}

impl MediaSource {
    pub(crate) async fn sample(&self) -> Result<MediaData, MediaError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Sample(reply))?;
        await_reply(response).await
    }

    pub(crate) async fn action(
        &self,
        session_id: Option<&str>,
        action: MediaAction,
    ) -> Result<(), MediaError> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Action {
            session_id: session_id.map(str::to_string),
            action,
            reply,
        })?;
        await_reply(response).await
    }

    fn send(&self, request: Request) -> Result<(), MediaError> {
        let mut slot = lock_unpoisoned(&self.requests);
        let request = match slot.as_ref() {
            Some(sender) => match sender.send(request) {
                Ok(()) => return Ok(()),
                // The worker thread is gone; start a fresh one for this request.
                Err(mpsc::SendError(request)) => request,
            },
            None => request,
        };
        let sender = spawn()?;
        sender
            .send(request)
            .map_err(|_| MediaError::WorkerUnavailable)?;
        *slot = Some(sender);
        Ok(())
    }
}

fn spawn() -> Result<mpsc::Sender<Request>, MediaError> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("smabar-media".to_string())
        .spawn(move || worker(receiver))
        .map_err(|error| backend("failed to start the Windows media worker", error))?;
    Ok(sender)
}

async fn await_reply<T>(
    response: oneshot::Receiver<Result<T, MediaError>>,
) -> Result<T, MediaError> {
    match tokio::time::timeout(REPLY_TIMEOUT, response).await {
        Ok(Ok(result)) => result,
        Ok(Err(_dropped)) => Err(MediaError::WorkerUnavailable),
        Err(_elapsed) => Err(MediaError::Timeout),
    }
}

pub(super) fn backend(context: &str, error: impl std::fmt::Display) -> MediaError {
    MediaError::Backend(format!("{context}: {error}"))
}

fn worker(requests: mpsc::Receiver<Request>) {
    let _com = match ComGuard::initialize() {
        Ok(guard) => guard,
        Err(error) => {
            for request in requests {
                reply_error(request, error.clone());
            }
            return;
        }
    };
    let mut worker = Worker::default();
    for request in requests {
        match request {
            Request::Sample(reply) => {
                let _ = reply.send(worker.sample());
            }
            Request::Action {
                session_id,
                action,
                reply,
            } => {
                let _ = reply.send(worker.action(session_id.as_deref(), action));
            }
        }
    }
}

fn reply_error(request: Request, error: MediaError) {
    match request {
        Request::Sample(reply) => {
            let _ = reply.send(Err(error));
        }
        Request::Action { reply, .. } => {
            let _ = reply.send(Err(error));
        }
    }
}

struct ComGuard;

impl ComGuard {
    fn initialize() -> Result<Self, MediaError> {
        // SAFETY: this dedicated worker owns the matching CoUninitialize in
        // Drop and no WinRT object leaves the thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| backend("failed to initialize Windows COM", error))?;
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        // SAFETY: paired with the successful CoInitializeEx on this thread.
        unsafe { CoUninitialize() };
    }
}
