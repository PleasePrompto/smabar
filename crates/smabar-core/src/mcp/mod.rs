//! Embedded MCP server (Streamable HTTP on 127.0.0.1) so local LLM agents
//! can manage the whole bar: plugin CRUD + logs, bar position, settings, tile
//! order, provider snapshot.
//!
//! The tools share the same `smabar-core` logic layer as the Tauri commands;
//! mutations go through [`ConfigWatcher::apply`] so every live consumer
//! (shell events, plugin settings fan-out) reacts as if the config file had
//! been edited by hand.

mod bar_tools;
mod capture_tools;
mod command_tools;
mod data_tools;
mod font_tools;
mod guide_tools;
mod instructions;
mod lifecycle_tools;
mod plugin_reload;
mod plugin_tools;
mod plugin_types;
mod shortcut_tools;
mod store_tools;
mod theme_tools;
mod types;
mod ui_kit_tools;

#[cfg(test)]
mod capability_tests;
#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod data_tests;
#[cfg(test)]
mod description_tests;
#[cfg(test)]
mod font_tests;
#[cfg(test)]
mod guide_start_tests;
#[cfg(test)]
mod guide_template_tests;
#[cfg(test)]
mod guide_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod log_tests;
#[cfg(test)]
mod plugin_start_tests;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod shortcut_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod theme_tests;
#[cfg(test)]
mod ui_kit_cover_tests;
#[cfg(test)]
mod ui_kit_index_tests;
#[cfg(test)]
mod ui_kit_lookup_tests;
#[cfg(test)]
mod ui_kit_sections_tests;
#[cfg(test)]
mod ui_kit_tests;

use std::net::SocketAddr;
use std::sync::Arc;

use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
use rmcp::model::{Implementation, PromptMessage, Role, ServerCapabilities, ServerInfo};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ServerHandler, prompt, prompt_handler, prompt_router, tool_handler};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::capture::BarPort;
use crate::config::{ConfigWatcher, SmabarPaths};
use crate::plugins::PluginSupervisor;
use crate::providers::ProviderHub;
use crate::shortcuts::ShortcutsService;

/// Errors from starting the embedded MCP server.
#[derive(Debug, Error)]
pub enum McpServeError {
    /// The localhost listener could not be bound (port in use, no permission).
    #[error(
        "cannot bind the MCP server to 127.0.0.1:{port}: {source}; the port is configurable via \"mcp.port\" in config.json"
    )]
    Bind {
        port: u16,
        #[source]
        source: std::io::Error,
    },
}

/// The MCP handler: one instance per HTTP session, all cloning the same
/// shared core state.
#[derive(Clone)]
pub struct SmabarMcp {
    paths: SmabarPaths,
    config: Arc<ConfigWatcher>,
    hub: ProviderHub,
    supervisor: PluginSupervisor,
    shortcuts: ShortcutsService,
    bar: BarPort,
    /// The Community Store client; `None` in tests that never touch it.
    store: Option<crate::store::StoreService>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

#[prompt_router]
impl SmabarMcp {
    pub fn new(
        paths: SmabarPaths,
        config: Arc<ConfigWatcher>,
        hub: ProviderHub,
        supervisor: PluginSupervisor,
        shortcuts: ShortcutsService,
        bar: BarPort,
    ) -> Self {
        Self {
            paths,
            config,
            hub,
            supervisor,
            shortcuts,
            bar,
            store: None,
            tool_router: Self::plugin_tool_router()
                + Self::command_tool_router()
                + Self::lifecycle_tool_router()
                + Self::data_tool_router()
                + Self::font_tool_router()
                + Self::bar_tool_router()
                + Self::theme_tool_router()
                + Self::shortcut_tool_router()
                + Self::ui_kit_tool_router()
                + Self::guide_tool_router()
                + Self::capture_tool_router()
                + Self::store_tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Wires the Community Store client so the `store_*` tools work.
    #[must_use]
    pub fn with_store(mut self, store: crate::store::StoreService) -> Self {
        self.store = Some(store);
        self
    }

    #[prompt(
        name = "design_plugin",
        description = "Design or revise a plugin tile and flyout using smabar's covers, UI kit, best practices, logs and visual screenshot loop."
    )]
    fn design_plugin(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            instructions::design_plugin_prompt(&guide_tools::GUIDE["goldenPath"]),
        )]
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for SmabarMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("smabar", env!("CARGO_PKG_VERSION")))
        .with_instructions(instructions::INSTRUCTIONS)
    }
}

/// Handle of a running MCP server. Dropping it detaches the server (it keeps
/// serving for the process lifetime); [`McpServer::shutdown`] stops it
/// gracefully (used by tests).
pub struct McpServer {
    addr: SocketAddr,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl McpServer {
    /// The bound local address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Gracefully stops the server: terminates active sessions and waits for
    /// the serve task to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        if let Err(error) = self.task.await {
            tracing::warn!(%error, "MCP server task ended abnormally");
        }
    }
}

/// Serves `handler` as a Streamable-HTTP MCP endpoint at
/// `http://127.0.0.1:<port>/mcp`. Host-header validation stays on the rmcp
/// default (loopback only) and Origin validation is restricted to localhost
/// origins, so browser pages and DNS-rebinding hosts cannot reach the server.
pub async fn serve(handler: SmabarMcp, port: u16) -> Result<McpServer, McpServeError> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|source| McpServeError::Bind { port, source })?;
    let addr = listener
        .local_addr()
        .map_err(|source| McpServeError::Bind { port, source })?;

    let cancel = CancellationToken::new();
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        LocalSessionManager::default().into(),
        http_config(cancel.clone(), port),
    );
    let router = axum::Router::new().nest_service("/mcp", service);

    let shutdown = cancel.clone();
    let task = tokio::spawn(async move {
        let served = axum::serve(listener, router)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await;
        if let Err(error) = served {
            tracing::error!(%error, "MCP server ended with an error");
        }
    });
    Ok(McpServer { addr, cancel, task })
}

fn http_config(cancel: CancellationToken, port: u16) -> StreamableHttpServerConfig {
    let mut config = StreamableHttpServerConfig::default();
    config.cancellation_token = cancel;
    // Non-browser MCP clients send no Origin header and always pass; this
    // only rejects cross-origin browser requests (empty would disable the
    // check entirely).
    config.allowed_origins = vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
    ];
    config
}
