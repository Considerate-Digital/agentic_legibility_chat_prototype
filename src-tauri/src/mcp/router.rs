//! Router that multiplexes multiple MCP servers behind a single dispatch API.
//!
//! Tool names returned to the LLM are server-namespaced (`server::tool`) so
//! two servers can expose same-named tools without colliding. The router's
//! `dispatch` parses `server::tool`, looks up the right server, strips the
//! prefix, and forwards.
//!
//! Auto-fetched tools (from a server's `tools/list`) are converted to
//! `LLMToolDef` records by `namespaced_tools` below.
//!
//! `McpClientEnum` keeps a `Stdio`/`Http` split even though only `Stdio` is
//! populated today (the single `legibility-chat-mcp` sidecar) — this preserves
//! room for future third-party MCP servers without re-widening the enum.

use anyhow::{anyhow, Result};

use crate::llm::types::{LLMFunctionDef, LLMToolDef};

use super::legibility_chat_client::LegibilityChatClient;

pub enum McpClientEnum {
    Stdio(LegibilityChatClient),
    /// No HTTP-transport MCP server is wired up today (the `legibility-mcp`
    /// HTTP sidecar was retired in favour of natively-ported tools in
    /// `legibility-chat-mcp`), but the variant is kept — uninhabited via
    /// `Infallible` — so a future third-party MCP server can slot in here
    /// without reshaping the router.
    Http(std::convert::Infallible),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    /// Loaded from a local `.md` file under `tools/<server>/`.
    Markdown,
    /// Fetched from the upstream server's `tools/list` at startup.
    Fetched,
}

pub struct McpServerHandle {
    pub name: String,
    pub client: McpClientEnum,
    pub tools: Vec<LLMToolDef>,
    pub source: ToolSource,
}

pub struct McpRouter {
    servers: Vec<McpServerHandle>,
}

impl McpRouter {
    pub fn from_servers(servers: Vec<McpServerHandle>) -> Self {
        Self { servers }
    }

    /// Take the router apart so callers can rebuild it with one server
    /// replaced (used by `set_live_resources_dir`).
    pub fn into_servers(self) -> Vec<McpServerHandle> {
        self.servers
    }

    /// Concatenate every server's namespaced tools.
    pub fn all_tools(&self) -> Vec<LLMToolDef> {
        self.servers.iter().flat_map(|s| s.tools.iter().cloned()).collect()
    }

    /// Route a call to the server that owns the named tool.
    ///
    /// The LLM sees bare tool names (e.g. `get_service`, `get_endpoint`),
    /// so we look up ownership by matching `function.name` across every
    /// server's known tools. Throws if no server owns the name.
    pub async fn dispatch(&self, name: &str, args: serde_json::Value) -> Result<String> {
        let server = self
            .servers
            .iter()
            .find(|s| s.tools.iter().any(|t| t.function.name == name))
            .ok_or_else(|| {
                let known: Vec<&str> = self
                    .servers
                    .iter()
                    .flat_map(|s| s.tools.iter().map(|t| t.function.name.as_str()))
                    .collect();
                anyhow!(
                    "no MCP server owns tool '{}' (known tools: {:?})",
                    name,
                    known
                )
            })?;

        match &server.client {
            McpClientEnum::Stdio(c) => c.call_tool(name, args).await,
            McpClientEnum::Http(never) => match *never {},
        }
    }

    /// Server names currently registered.
    pub fn server_names(&self) -> Vec<&str> {
        self.servers.iter().map(|s| s.name.as_str()).collect()
    }

    /// Tool names (unprefixed) for the given server.
    pub fn tools_for(&self, server: &str) -> Vec<String> {
        self.servers
            .iter()
            .find(|s| s.name == server)
            .map(|s| s.tools.iter().map(|t| t.function.name.clone()).collect())
            .unwrap_or_default()
    }
}

/// Find the server that owns a given bare tool name. Returns the server
/// name (e.g. "state") for use in the UI's `tool-called` badge.
pub fn find_server_for_tool<'a>(router: &'a McpRouter, bare_name: &str) -> Option<&'a str> {
    router
        .servers
        .iter()
        .find(|s| s.tools.iter().any(|t| t.function.name == bare_name))
        .map(|s| s.name.as_str())
}

/// Convert a server's `tools/list` response into our internal
/// `LLMToolDef` records. We store the bare tool name (`get_endpoint`,
/// `list_services`, …) here — the router resolves server ownership via
/// `find_server_for_tool` at dispatch time. Input shape (rmcp):
///
/// ```json
/// {
///   "tools": [
///     { "name": "get_endpoint", "description": "...",
///       "inputSchema": { "type": "object", "properties": {...}, "required": [...] } }
///   ]
/// }
/// ```
pub fn namespaced_tools(server: &str, list_result: &serde_json::Value) -> Vec<LLMToolDef> {
    let Some(tools) = list_result.get("tools").and_then(|t| t.as_array()) else {
        return Vec::new();
    };

    tools
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?;
            let description = t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            // rmcp uses inputSchema; some servers use parameters. Accept both.
            let parameters = t
                .get("inputSchema")
                .or_else(|| t.get("parameters"))
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::json!({ "type": "object", "properties": {} })
                });
            let _ = server; // server ownership is recorded on the parent handle.
            Some(LLMToolDef {
                def_type: "function".into(),
                function: LLMFunctionDef {
                    name: name.to_string(),
                    description: description.to_string(),
                    parameters,
                },
            })
        })
        .collect()
}
