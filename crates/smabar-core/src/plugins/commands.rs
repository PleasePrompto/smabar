//! Discoverable, acknowledged calls to a running plugin. The reader stays free
//! while a caller waits, including when the handler calls back into the host.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::rpc::{MAX_LINE_BYTES, RpcClient};
use super::{PluginError, PluginSupervisor};
use crate::util::lock_unpoisoned;

pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCommandInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
}

pub(crate) fn parse_commands(reply: &Value) -> Result<Vec<PluginCommandInfo>, String> {
    let commands: Vec<PluginCommandInfo> =
        serde_json::from_value(reply.get("commands").cloned().unwrap_or_else(|| json!([])))
            .map_err(|error| format!("invalid initialize.commands: {error}"))?;
    if commands.len() > 64 {
        return Err("initialize.commands supports at most 64 commands".into());
    }
    let mut names = std::collections::HashSet::new();
    for command in &commands {
        if !valid_name(&command.name)
            || !names.insert(&command.name)
            || command.description.is_empty()
            || command.description.len() > 4096
            || !command.input_schema.is_object()
            || !command.output_schema.is_object()
        {
            return Err("commands need unique 1–128 character names (letters, digits, '.', '_', '-'), a description, and object schemas".into());
        }
    }
    Ok(commands)
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[derive(Clone)]
pub(crate) struct Session {
    pub generation: u64,
    pub rpc: RpcClient,
    pub commands: Vec<PluginCommandInfo>,
    pub stopped: CancellationToken,
}

#[derive(Default)]
pub(crate) struct Sessions {
    next: AtomicU64,
    pub active: Mutex<HashMap<String, Session>>,
}

pub(crate) struct SessionGuard {
    sessions: Arc<Sessions>,
    plugin: String,
    pub session: Session,
}

impl Sessions {
    pub fn enter(
        self: &Arc<Self>,
        plugin: &str,
        rpc: RpcClient,
        commands: Vec<PluginCommandInfo>,
    ) -> SessionGuard {
        let session = Session {
            generation: self.next.fetch_add(1, Ordering::Relaxed) + 1,
            rpc,
            commands,
            stopped: CancellationToken::new(),
        };
        if let Some(old) = lock_unpoisoned(&self.active).insert(plugin.into(), session.clone()) {
            old.stopped.cancel();
        }
        SessionGuard {
            sessions: self.clone(),
            plugin: plugin.into(),
            session,
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.session.stopped.cancel();
        let mut active = lock_unpoisoned(&self.sessions.active);
        if active
            .get(&self.plugin)
            .is_some_and(|s| s.generation == self.session.generation)
        {
            active.remove(&self.plugin);
        }
    }
}

impl PluginSupervisor {
    fn command_session(&self, plugin: &str) -> Result<Session, PluginError> {
        lock_unpoisoned(&self.inner.sessions.active)
            .get(plugin)
            .cloned()
            .ok_or_else(|| PluginError::NotRunning { id: plugin.into() })
    }

    pub fn commands(&self, plugin: &str) -> Result<Vec<PluginCommandInfo>, PluginError> {
        Ok(self.command_session(plugin)?.commands)
    }

    pub async fn call(
        &self,
        plugin: &str,
        command: &str,
        arguments: Value,
    ) -> Result<Value, PluginError> {
        let session = self.command_session(plugin)?;
        if !session.commands.iter().any(|item| item.name == command) {
            return Err(PluginError::Command {
                message: "unknown command; use plugin_commands to discover supported commands"
                    .into(),
            });
        }
        if !arguments.is_object() || arguments.to_string().len() > MAX_LINE_BYTES / 2 {
            return Err(PluginError::Command {
                message: "command arguments must be an object of at most 512 KiB".into(),
            });
        }
        tokio::select! {
            _ = session.stopped.cancelled() => Err(PluginError::Command { message: "plugin stopped during command; outcome unknown, query state before retrying".into() }),
            result = session.rpc.request("command.call", json!({"command": command, "arguments": arguments}), COMMAND_TIMEOUT) => {
                result.map_err(|error| PluginError::Command { message: error.to_string() })
            }
        }
    }
}
