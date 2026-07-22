//! stdio MCP client for the `legibility-chat-mcp` sidecar (formerly `McpClient`).
//!
//! Speaks newline-delimited JSON-RPC 2.0 over the sidecar's stdin/stdout.
//! The Tauri shell plugin owns the child process lifecycle.

use anyhow::{anyhow, Context, Result};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

pub struct LegibilityChatClient {
    inner: tokio::sync::Mutex<LegibilityChatClientInner>,
}

struct LegibilityChatClientInner {
    child: Option<tauri_plugin_shell::process::CommandChild>,
    rx: tokio::sync::mpsc::Receiver<CommandEvent>,
    buf: String,
    next_id: u64,
}

impl LegibilityChatClient {
    pub async fn new(app: &tauri::AppHandle, live_resources_dir: Option<&str>) -> Result<Self> {
        let mut sidecar = app
            .shell()
            .sidecar("legibility-chat-mcp")
            .context("resolving legibility-chat-mcp sidecar")?;

        if let Some(dir) = live_resources_dir {
            sidecar = sidecar.env("LIVE_RESOURCES_DIR", dir);
        }

        let (rx, child) = sidecar.spawn().context("spawning legibility-chat-mcp sidecar")?;

        let mut client = LegibilityChatClientInner {
            child: Some(child),
            rx,
            buf: String::new(),
            next_id: 1,
        };

        // Perform the MCP initialize handshake
        client
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "legibility-chat", "version": "0.1.0" }
                }),
            )
            .await
            .context("MCP initialize")?;

        Ok(Self {
            inner: tokio::sync::Mutex::new(client),
        })
    }

    /// Call a tool on the legibility-chat-mcp server.
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<String> {
        let result = self
            .inner
            .lock()
            .await
            .request(
                "tools/call",
                serde_json::json!({
                    "name": name,
                    "arguments": args
                }),
            )
            .await?;

        // MCP tools/call result: { content: [{ type: "text", text: "..." }] }
        let text = result["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or("(no result)")
            .to_string();

        Ok(text)
    }

    /// Call `tools/list` on the legibility-chat-mcp server and return the raw
    /// JSON-RPC result (`{ "tools": [...] }`), the same shape
    /// `namespaced_tools` expects from any MCP server.
    pub async fn tools_list(&self) -> Result<serde_json::Value> {
        self.inner
            .lock()
            .await
            .request("tools/list", serde_json::json!({}))
            .await
    }

    /// Kill the spawned legibility-chat-mcp sidecar process. Used when
    /// rebuilding the MCP router (`live_resources_dir` changed) so the old
    /// sidecar doesn't linger after a fresh one is spawned with the new env.
    pub async fn shutdown(&self) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if let Some(child) = inner.child.take() {
            child.kill().context("killing legibility-chat-mcp sidecar")?;
        }
        Ok(())
    }
}

impl LegibilityChatClientInner {
    async fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        let line = req + "\n";

        self.child
            .as_mut()
            .ok_or_else(|| anyhow!("legibility-chat-mcp sidecar already shut down"))?
            .write(line.as_bytes())
            .context("writing to legibility-chat-mcp sidecar stdin")?;

        loop {
            if let Some(pos) = self.buf.find('\n') {
                let response_line = self.buf[..pos].to_string();
                self.buf = self.buf[pos + 1..].to_string();

                if response_line.trim().is_empty() {
                    continue;
                }

                let val: serde_json::Value = serde_json::from_str(&response_line)
                    .context("parsing MCP response")?;

                if val.get("id").and_then(|v| v.as_u64()) == Some(id) {
                    if let Some(error) = val.get("error") {
                        return Err(anyhow!("MCP error: {}", error));
                    }
                    return Ok(val["result"].clone());
                }
                continue;
            }

            match self.rx.recv().await {
                Some(CommandEvent::Stdout(bytes)) => {
                    self.buf.push_str(&String::from_utf8_lossy(&bytes));
                }
                Some(CommandEvent::Terminated(_)) | None => {
                    return Err(anyhow!("legibility-chat-mcp sidecar terminated unexpectedly"));
                }
                _ => {}
            }
        }
    }
}
