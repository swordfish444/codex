//! Connection manager for Model Context Protocol (MCP) servers.
//!
//! The [`McpConnectionManager`] owns one [`codex_mcp_client::McpClient`] per
//! configured server (keyed by the *server name*). It offers convenience
//! helpers to query the available tools across *all* servers and returns them
//! in a single aggregated map using the fully-qualified tool name
//! `"<server><MCP_TOOL_NAME_DELIMITER><tool>"` as the key.

use std::collections::HashMap;

use anyhow::anyhow;
use anyhow::Result;
use codex_mcp_client::McpClient;
use mcp_types::Tool;
use tokio::task::JoinSet;
use tracing::info;
use tracing::warn;

use crate::mcp_server_config::McpServerConfig;

/// Delimiter used to separate the server name from the tool name in a fully
/// qualified tool name.
///
/// OpenAI requires tool names to conform to `^[a-zA-Z0-9_-]+$`, so we must
/// choose a delimiter from this character set.
const MCP_TOOL_NAME_DELIMITER: &str = "__OAI_CODEX_MCP__";

fn fully_qualified_tool_name(server: &str, tool: &str) -> String {
    format!("{server}{MCP_TOOL_NAME_DELIMITER}{tool}")
}

pub(crate) fn try_parse_fully_qualified_tool_name(fq_name: &str) -> Option<(String, String)> {
    let (server, tool) = fq_name.split_once(MCP_TOOL_NAME_DELIMITER)?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

/// A thin wrapper around a set of running [`McpClient`] instances.
///
/// The struct is intentionally lightweight – cloning just clones the internal
/// `HashMap` of clients which in turn clones the `Arc`s of each client.
#[derive(Clone)]
pub(crate) struct McpConnectionManager {
    /// Server-name → client instance.
    ///
    /// The server name originates from the keys of the `mcp_servers` map in
    /// the user configuration.
    clients: HashMap<String, std::sync::Arc<McpClient>>, // Arc to cheaply clone
}

impl McpConnectionManager {
    /// Spawn a [`McpClient`] for each configured server.
    ///
    /// * `mcp_servers` – Map loaded from the user configuration where *keys*
    ///   are human-readable server identifiers and *values* are the spawn
    ///   instructions.
    pub async fn new(mcp_servers: HashMap<String, McpServerConfig>) -> Result<Self> {
        // Early exit if no servers are configured.
        if mcp_servers.is_empty() {
            return Ok(Self {
                clients: HashMap::new(),
            });
        }

        // Spin up all servers concurrently.
        let mut join_set = JoinSet::new();

        // Spawn tasks to launch each server.
        for (server_name, cfg) in mcp_servers {
            // Perform slash validation up-front so we can return early without
            // spawning any tasks when the name is invalid.
            if server_name.contains('/') {
                return Err(anyhow!(
                    "MCP server name '{server_name}' must not contain a forward slash (/)"
                ));
            }

            join_set.spawn(async move {
                // Build argv vector: first element is the command itself followed
                // by the optional additional args from the config.
                let mut argv = vec![cfg.command.clone()];
                argv.extend(cfg.args.clone());

                let client_res = McpClient::new_stdio_client(argv).await;

                (server_name, client_res)
            });
        }

        // Collect results.
        let mut clients: HashMap<String, std::sync::Arc<McpClient>> = HashMap::new();

        while let Some(res) = join_set.join_next().await {
            let (server_name, client_res) = res?; // propagate JoinError

            let client = client_res
                .map_err(|e| anyhow!("failed to spawn MCP server '{server_name}': {e}"))?;

            clients.insert(server_name, std::sync::Arc::new(client));
        }

        Ok(Self { clients })
    }

    /// Return a reference to the internal client for the given server.
    #[allow(dead_code)]
    pub fn client_for_server(&self, server_name: &str) -> Option<std::sync::Arc<McpClient>> {
        self.clients.get(server_name).cloned()
    }

    /// Query every server for its available tools and return a single map that
    /// contains **all** tools.  The key is the fully-qualified name
    /// `<server>/<tool>`.
    pub async fn list_all_tools(&self) -> Result<HashMap<String, Tool>> {
        let mut join_set = JoinSet::new();

        // Spawn one task per server so we can query them concurrently. This
        // keeps the overall latency roughly at the slowest server instead of
        // the cumulative latency.
        for (server_name, client) in self.clients.clone() {
            let server_name_cloned = server_name.clone();
            let client_clone = client.clone();
            join_set.spawn(async move {
                let res = client_clone.list_tools(None).await;
                (server_name_cloned, res)
            });
        }

        let mut aggregated: HashMap<String, Tool> = HashMap::new();

        while let Some(join_res) = join_set.join_next().await {
            let (server_name, list_result) = join_res?; // propagate JoinError

            let list_result = list_result?;

            for tool in list_result.tools {
                if tool.name.contains('/') {
                    warn!(
                        server = %server_name,
                        tool_name = %tool.name,
                        "tool name contains '/' – skipping to avoid ambiguity"
                    );
                    continue;
                }

                let fq_name = fully_qualified_tool_name(&server_name, &tool.name);

                if aggregated.insert(fq_name.clone(), tool).is_some() {
                    warn!("tool name collision for '{fq_name}' – overwriting previous entry");
                }
            }
        }

        info!(
            "aggregated {} tools from {} servers",
            aggregated.len(),
            self.clients.len()
        );

        Ok(aggregated)
    }

    /// Route a fully-qualified tool call to the matching server.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<mcp_types::CallToolResult> {
        let client = self
            .clients
            .get(server)
            .ok_or_else(|| anyhow!("unknown MCP server '{server}'"))?
            .clone();

        client
            .call_tool(tool.to_string(), arguments)
            .await
            .map_err(|e| anyhow!("tool call failed for '{server}/{tool}': {e}"))
    }
}

/// Convenience helper that mirrors the previous `create_mcp_connection_manager`
/// free-standing function but returns `Result` and is **async**. Existing
/// call-sites can continue to call the function while new code can use the
/// `McpConnectionManager::new` associated function directly.
pub(crate) async fn create_mcp_connection_manager(
    mcp_servers: HashMap<String, McpServerConfig>,
) -> Result<McpConnectionManager> {
    McpConnectionManager::new(mcp_servers).await
}
