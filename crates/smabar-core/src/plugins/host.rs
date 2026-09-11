//! Bounded plugin requests to desktop services. No Tauri dependency in core.

use super::rpc::RpcClient;
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct HostPort(mpsc::Sender<HostRequest>);

/// Identity is assigned by the supervisor, never accepted from plugin JSON.
#[derive(Clone)]
pub struct HostSession {
    pub plugin_id: String,
    pub generation: u64,
    pub plugin_dir: PathBuf,
    pub data_dir: PathBuf,
    pub tiles: Vec<String>,
    pub stopped: CancellationToken,
    pub(crate) rpc: RpcClient,
}

impl HostSession {
    pub async fn notify(&self, method: &str, params: Value) -> bool {
        if self.stopped.is_cancelled() {
            return false;
        }
        if !self.rpc.notify(method, params).await {
            tracing::debug!(plugin = %self.plugin_id, method, "plugin stopped before service event delivery");
            return false;
        }
        true
    }
}

pub struct HostRequest {
    pub session: HostSession,
    pub method: String,
    pub params: Value,
    pub reply: oneshot::Sender<Result<Value, String>>,
}

impl HostPort {
    pub fn channel() -> (Self, mpsc::Receiver<HostRequest>) {
        let (tx, rx) = mpsc::channel(64);
        (Self(tx), rx)
    }

    pub(crate) async fn request(
        &self,
        session: HostSession,
        method: &str,
        params: &Value,
    ) -> Result<Value, String> {
        let stopped = session.stopped.clone();
        let (reply, receiver) = oneshot::channel();
        self.0
            .try_send(HostRequest {
                session,
                method: method.into(),
                params: params.clone(),
                reply,
            })
            .map_err(|_| "desktop service queue is full or unavailable; retry later".to_string())?;
        tokio::select! {
            _ = stopped.cancelled() => Err("plugin session ended".into()),
            result = tokio::time::timeout(std::time::Duration::from_secs(8), receiver) => {
                result.map_err(|_| "desktop service timed out; query state before retrying".to_string())?
                    .map_err(|_| "desktop service stopped".to_string())?
            }
        }
    }
}
