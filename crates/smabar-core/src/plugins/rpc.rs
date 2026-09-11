//! Per-plugin JSON-RPC plumbing: request/response matching plus the writer
//! and reader tasks around the child process's stdio.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::protocol::{self, IncomingLine};
use crate::util::lock_unpoisoned;

pub(crate) const MAX_LINE_BYTES: usize = 1024 * 1024;
pub(crate) const CHANNEL_CAPACITY: usize = 64;
const MAX_PENDING_REQUESTS: usize = 32;

/// Failure of a core→plugin request.
#[derive(Debug, Error)]
pub(crate) enum RpcCallError {
    #[error("plugin answered with an error: {0}")]
    Remote(String),
    #[error("plugin connection closed before it responded")]
    Closed,
    #[error("plugin did not respond within {0:?}; outcome unknown, query state before retrying")]
    Timeout(Duration),
    #[error("plugin already has {0} requests in flight; wait for a response before sending more")]
    Busy(usize),
}

type PendingSender = oneshot::Sender<Result<Value, String>>;

/// In-flight core→plugin requests, keyed by request id.
#[derive(Clone, Default)]
pub(crate) struct PendingMap {
    inner: Arc<Mutex<HashMap<u64, PendingSender>>>,
}

impl PendingMap {
    fn insert(&self, id: u64, sender: PendingSender) -> bool {
        let mut pending = lock_unpoisoned(&self.inner);
        if pending.len() >= MAX_PENDING_REQUESTS {
            return false;
        }
        pending.insert(id, sender);
        true
    }

    fn remove(&self, id: u64) {
        lock_unpoisoned(&self.inner).remove(&id);
    }

    fn complete(&self, id: u64, result: Result<Value, String>) {
        match lock_unpoisoned(&self.inner).remove(&id) {
            // Err means the requester gave up (timeout) — nothing to do.
            Some(sender) => {
                let _ = sender.send(result);
            }
            None => tracing::debug!(id, "plugin answered an unknown request id"),
        }
    }

    /// Fails every in-flight request by dropping its sender; the waiters see
    /// [`RpcCallError::Closed`]. Called when the plugin's stdout closes.
    fn fail_all(&self) {
        lock_unpoisoned(&self.inner).clear();
    }
}

/// Cheap-to-clone sender half of one plugin's JSON-RPC connection.
#[derive(Clone)]
pub(crate) struct RpcClient {
    out_tx: mpsc::Sender<String>,
    pending: PendingMap,
    next_id: Arc<AtomicU64>,
}

impl RpcClient {
    pub(crate) fn new(out_tx: mpsc::Sender<String>, pending: PendingMap) -> Self {
        Self {
            out_tx,
            pending,
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Sends a request and awaits its response.
    pub(crate) async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcCallError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = oneshot::channel();
        if !self.pending.insert(id, sender) {
            return Err(RpcCallError::Busy(MAX_PENDING_REQUESTS));
        }
        let line = protocol::request_line(id, method, params);
        let response = tokio::time::timeout(timeout, async {
            self.out_tx
                .send(line)
                .await
                .map_err(|_| RpcCallError::Closed)?;
            receiver.await.map_err(|_| RpcCallError::Closed)
        })
        .await;
        let result = match response {
            Err(_) => Err(RpcCallError::Timeout(timeout)),
            Ok(Err(error)) => Err(error),
            Ok(Ok(Err(message))) => Err(RpcCallError::Remote(message)),
            Ok(Ok(Ok(value))) => Ok(value),
        };
        if result.is_err() {
            self.pending.remove(id);
        }
        result
    }

    /// Queues a notification and reports whether the writer still accepts it.
    pub(crate) async fn notify(&self, method: &str, params: Value) -> bool {
        let line = protocol::notification_line(method, params);
        if self.out_tx.send(line).await.is_err() {
            tracing::debug!(method, "notification dropped; plugin connection closed");
            return false;
        }
        true
    }

    /// Answers a plugin→core request successfully.
    pub(crate) async fn respond_ok(&self, id: &Value, result: Value) {
        let _ = self
            .out_tx
            .send(protocol::response_ok_line(id, result))
            .await;
    }

    /// Answers a plugin→core request with an error.
    pub(crate) async fn respond_error(&self, id: &Value, code: i64, message: &str) {
        let line = protocol::response_error_line(id, code, message);
        let _ = self.out_tx.send(line).await;
    }
}

/// Messages surfaced to the plugin's supervision loop.
#[derive(Debug)]
pub(crate) enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// Non-JSON-RPC stdout line (auto-captured as an info log entry).
    Stdout(String),
    /// stderr line (auto-captured as a warn log entry).
    Stderr(String),
    /// One complete output line exceeded [`MAX_LINE_BYTES`] and was discarded.
    LineTooLong {
        source: &'static str,
    },
    /// The plugin closed its stdout — it exited or is dying.
    StdoutClosed,
}

/// Writer task: sends each queued line, newline-terminated, to stdin.
pub(crate) fn spawn_writer(
    mut stdin: ChildStdin,
    mut rx: mpsc::Receiver<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if let Err(error) = write_line(&mut stdin, &line).await {
                // The process is dead; the stdout reader reports it.
                tracing::debug!(%error, "plugin stdin write failed; stopping writer");
                break;
            }
        }
    })
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> std::io::Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

/// Reader task over stdout: completes pending requests directly and forwards
/// everything else to the supervision loop. Sends [`Incoming::StdoutClosed`]
/// (after failing all pending requests) when the stream ends.
pub(crate) fn spawn_stdout_reader(
    stdout: ChildStdout,
    pending: PendingMap,
    tx: mpsc::Sender<Incoming>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            match read_bounded_line(&mut reader, &mut buffer).await {
                Ok(BoundedLine::Line(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let forwarded = match protocol::classify_line(&line) {
                        IncomingLine::Response { id, result } => {
                            pending.complete(id, result);
                            continue;
                        }
                        IncomingLine::Request { id, method, params } => {
                            Incoming::Request { id, method, params }
                        }
                        IncomingLine::Notification { method, params } => {
                            Incoming::Notification { method, params }
                        }
                        IncomingLine::NotRpc => Incoming::Stdout(line),
                    };
                    if tx.send(forwarded).await.is_err() {
                        break;
                    }
                }
                Ok(BoundedLine::TooLong) => {
                    if tx
                        .send(Incoming::LineTooLong { source: "stdout" })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(BoundedLine::Eof) => break,
                Err(error) => {
                    tracing::debug!(%error, "plugin stdout read failed");
                    break;
                }
            }
        }
        pending.fail_all();
        // Err just means the supervision loop is already gone.
        let _ = tx.send(Incoming::StdoutClosed).await;
    })
}

/// Reader task over stderr: every non-empty line becomes a log capture.
pub(crate) fn spawn_stderr_reader(
    stderr: ChildStderr,
    tx: mpsc::Sender<Incoming>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buffer = Vec::new();
        loop {
            match read_bounded_line(&mut reader, &mut buffer).await {
                Ok(BoundedLine::Line(line)) if line.trim().is_empty() => {}
                Ok(BoundedLine::Line(line)) => {
                    if tx.send(Incoming::Stderr(line)).await.is_err() {
                        break;
                    }
                }
                Ok(BoundedLine::TooLong) => {
                    if tx
                        .send(Incoming::LineTooLong { source: "stderr" })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(BoundedLine::Eof) => break,
                Err(error) => {
                    tracing::debug!(%error, "plugin stderr read failed");
                    break;
                }
            }
        }
    })
}

#[derive(Debug, PartialEq, Eq)]
enum BoundedLine {
    Line(String),
    TooLong,
    Eof,
}

async fn read_bounded_line<R>(reader: &mut R, buffer: &mut Vec<u8>) -> std::io::Result<BoundedLine>
where
    R: AsyncBufRead + Unpin,
{
    buffer.clear();
    let mut too_long = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if buffer.is_empty() && !too_long {
                BoundedLine::Eof
            } else {
                finish_line(buffer, too_long)
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let content_end = newline.unwrap_or(available.len());
        if !too_long {
            let keep = content_end.min(MAX_LINE_BYTES.saturating_sub(buffer.len()));
            buffer.extend_from_slice(&available[..keep]);
            too_long = keep < content_end;
        }
        let consumed = newline.map_or(available.len(), |index| index + 1);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(finish_line(buffer, too_long));
        }
    }
}

fn finish_line(buffer: &mut Vec<u8>, too_long: bool) -> BoundedLine {
    if too_long {
        return BoundedLine::TooLong;
    }
    if buffer.last() == Some(&b'\r') {
        buffer.pop();
    }
    BoundedLine::Line(String::from_utf8_lossy(buffer).into_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn notify_reports_if_the_writer_queue_is_open() {
        let (sender, receiver) = mpsc::channel(1);
        let rpc = RpcClient::new(sender, PendingMap::default());

        assert!(rpc.notify("event", json!({"action": "refresh"})).await);
        drop(receiver);
        assert!(!rpc.notify("event", json!({"action": "refresh"})).await);
    }

    #[tokio::test]
    async fn oversized_lines_are_discarded_and_the_next_line_survives() {
        let input = format!("{}\nnext\r\n", "x".repeat(MAX_LINE_BYTES + 1));
        let mut reader = BufReader::new(input.as_bytes());
        let mut buffer = Vec::new();

        assert_eq!(
            read_bounded_line(&mut reader, &mut buffer)
                .await
                .expect("line"),
            BoundedLine::TooLong
        );
        assert_eq!(
            read_bounded_line(&mut reader, &mut buffer)
                .await
                .expect("line"),
            BoundedLine::Line("next".to_string())
        );
    }

    #[test]
    fn pending_requests_have_a_hard_limit() {
        let pending = PendingMap::default();
        for id in 0..MAX_PENDING_REQUESTS as u64 {
            let (sender, _receiver) = oneshot::channel();
            assert!(pending.insert(id, sender));
        }
        let (sender, _receiver) = oneshot::channel();
        assert!(!pending.insert(MAX_PENDING_REQUESTS as u64, sender));
    }
}
