use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

mod context;
mod protocol;
mod specs;
mod tools;

use context::AppContext;
use protocol::{
    InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo,
    ToolsCapability, ToolsListResult,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ctx = AppContext::from_env();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let stderr = tokio::io::stderr();

    let mut reader = BufReader::new(stdin).lines();
    let mut writer = tokio::io::BufWriter::new(stdout);
    let mut err_writer = tokio::io::BufWriter::new(stderr);

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => handle_request(req, &ctx).await,
            Err(e) => {
                let _ = err_writer
                    .write_all(format!("parse error: {e}\n").as_bytes())
                    .await;
                let _ = err_writer.flush().await;
                continue;
            }
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

async fn handle_request(req: JsonRpcRequest, ctx: &AppContext) -> JsonRpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "initialize" => JsonRpcResponse::result(
            id,
            serde_json::to_value(InitializeResult {
                protocol_version: "2024-11-05".into(),
                capabilities: ServerCapabilities {
                    tools: ToolsCapability { list_changed: false },
                },
                server_info: ServerInfo {
                    name: "legibility-chat-mcp".into(),
                    version: "0.1.0".into(),
                },
            })
            .unwrap(),
        ),
        "notifications/initialized" => {
            // Fire-and-forget notification — no response needed in JSON-RPC,
            // but we send an empty result to keep the protocol happy.
            JsonRpcResponse::result(id, serde_json::Value::Null)
        }
        "tools/list" => {
            let tool_list = ToolsListResult {
                tools: tools::all_tool_defs(ctx),
            };
            JsonRpcResponse::result(id, serde_json::to_value(tool_list).unwrap())
        }
        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args = params["arguments"].clone();
            let result = tools::call_tool(&name, args, ctx).await;
            JsonRpcResponse::result(id, serde_json::to_value(result).unwrap())
        }
        _ => JsonRpcResponse::error(
            id,
            -32601,
            format!("Method '{}' not found", req.method),
        ),
    }
}
