//! Smoke test for `LegibilityChatClient`.
//!
//! Spawns a real `legibility-chat-mcp` sidecar with `LIVE_RESOURCES_DIR` pointed
//! at `../../live_resources/` (relative to the state-machine workspace
//! root), calls `tools/list`, then dispatches `list_endpoints` and
//! `get_endpoint` to verify the stdio JSON-RPC round trip works — including
//! the spec-lookup tools natively ported into `legibility-chat-mcp` (formerly
//! served by the separate `legibility-mcp` HTTP sidecar).
//!
//! `LegibilityChatClient::new` spawns its sidecar through the Tauri shell plugin,
//! which needs a real `AppHandle`, so this boots a minimal Tauri app and
//! runs the smoke test from inside `.setup()`, exiting the process with the
//! test's result code when done.
//!
//! Run with: `cargo run -p legibility-chat --example legibility_chat_smoke`

use std::path::PathBuf;

use legibility_chat_lib::mcp::LegibilityChatClient;
use tauri::AppHandle;

fn main() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf();
    let specs_dir = workspace_root.join("live_resources");
    println!("→ Using specs dir: {}", specs_dir.display());
    if !specs_dir.is_dir() {
        eprintln!("specs dir does not exist; aborting");
        std::process::exit(1);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let specs_dir = specs_dir.clone();
            tauri::async_runtime::spawn(async move {
                let code = match run_smoke_test(&handle, &specs_dir).await {
                    Ok(()) => {
                        println!("✓ legibility-chat-mcp smoke test passed");
                        0
                    }
                    Err(e) => {
                        eprintln!("✗ legibility-chat-mcp smoke test failed: {e:#}");
                        1
                    }
                };
                handle.exit(code);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn run_smoke_test(handle: &AppHandle, specs_dir: &PathBuf) -> anyhow::Result<()> {
    println!("→ Spawning legibility-chat-mcp with LIVE_RESOURCES_DIR set…");
    let dir = specs_dir.to_string_lossy().to_string();
    let client = LegibilityChatClient::new(handle, Some(&dir)).await?;

    println!("→ Calling tools/list…");
    let list = client.tools_list().await?;
    let tools = list["tools"].as_array().cloned().unwrap_or_default();
    println!("  Found {} tools:", tools.len());
    for t in &tools {
        println!(
            "    - {} ({})",
            t["name"].as_str().unwrap_or("?"),
            t["description"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>()
        );
    }
    if tools.is_empty() {
        anyhow::bail!("zero tools returned — sidecar probably couldn't read specs");
    }

    println!("→ Calling list_endpoints…");
    let list_res = client
        .call_tool("list_endpoints", serde_json::json!({}))
        .await?;
    let endpoints: Vec<serde_json::Value> = serde_json::from_str(&list_res)?;
    let first = endpoints
        .first()
        .and_then(|e| e["name"].as_str().map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("no endpoints in index"))?;
    println!("  picked first endpoint: {first}");

    println!("→ Calling get_endpoint(\"{first}\")…");
    let res = client
        .call_tool("get_endpoint", serde_json::json!({ "name": first }))
        .await?;
    let preview: String = res.chars().take(200).collect();
    println!("  preview: {preview}…");

    client.shutdown().await.ok();
    Ok(())
}
