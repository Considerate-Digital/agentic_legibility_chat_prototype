//! Tauri commands for picking / setting the `live_resources/` directory used
//! by the spec-lookup tools natively exposed by `legibility-chat-mcp`. Restarting
//! the `legibility-chat-mcp` sidecar is the only way to change its
//! `LIVE_RESOURCES_DIR` env var, so `set_live_resources_dir` respawns the
//! "state" server via `build_router` and emits an event so the UI can
//! refresh.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::{build_router, ManagedState};

/// Counts of `.md` files under each spec-lookup subdirectory.
///
/// Returned by `scan_live_resources_dir` so the setup wizard can show a
/// preview ("Found 34 endpoints, 4 services, 1 plan") after the user
/// picks a directory.
#[derive(Debug, Clone, Serialize)]
pub struct LiveResourcesSummary {
    pub path: String,
    pub endpoints: usize,
    pub services: usize,
    pub plans: usize,
    /// True if at least one of the three subdirs contains `.md` files.
    pub is_valid: bool,
}

/// Count `.md` files directly under `base/<sub>/`. Returns 0 if the
/// subdir is missing or unreadable — callers treat 0+0+0 as "invalid".
fn count_md_files(base: &PathBuf, sub: &str) -> usize {
    let dir = base.join(sub);
    if !dir.is_dir() {
        return 0;
    }
    std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                .count()
        })
        .unwrap_or(0)
}

/// Scan a directory to confirm it has the layout the spec-lookup tools
/// expect (top-level `endpoints/`, `services/`, `plans/` subdirs containing
/// `.md` files). Returns counts so the wizard can show a preview.
#[tauri::command]
pub async fn scan_live_resources_dir(path: String) -> Result<LiveResourcesSummary, String> {
    let base = PathBuf::from(&path);
    if !base.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    let endpoints = count_md_files(&base, "endpoints");
    let services = count_md_files(&base, "services");
    let plans = count_md_files(&base, "plans");
    let is_valid = endpoints > 0 || services > 0 || plans > 0;

    Ok(LiveResourcesSummary {
        path,
        endpoints,
        services,
        plans,
        is_valid,
    })
}

/// Open a native folder picker. Returns `Some(path)` if the user picked a
/// directory, `None` if they cancelled.
#[tauri::command]
pub async fn pick_live_resources_dir(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |maybe| {
        let resolved = maybe.and_then(|fp| fp.into_path().ok()).map(|p: PathBuf| p.to_string_lossy().to_string());
        let _ = tx.send(resolved);
    });
    rx.await.map_err(|e| e.to_string())
}

/// Set (or clear) the live resources directory at runtime. Persists to
/// `AppConfig`, respawns the "state" server (`legibility-chat-mcp`) with the new
/// `LIVE_RESOURCES_DIR`, and emits `mcp-router-rebuilt`.
#[tauri::command]
pub async fn set_live_resources_dir(
    path: Option<String>,
    state: tauri::State<'_, ManagedState>,
    app: AppHandle,
) -> Result<(), String> {
    // 1. Validate.
    if let Some(p) = &path {
        let pb = PathBuf::from(p);
        if !pb.is_dir() {
            return Err(format!("Not a directory: {}", p));
        }
    }

    // 2. Persist.
    {
        let mut cfg = state.config.write().unwrap();
        cfg.live_resources_dir = path.clone();
        cfg.save().map_err(|e| e.to_string())?;
    }

    // 3. Rebuild the router. `build_router` always tears down and respawns
    // the "state" server so the sidecar picks up the new
    // `LIVE_RESOURCES_DIR` value — that requires a Tauri `AppHandle` to
    // spawn through the shell plugin, so we pass `Some(&app)`.
    let new_router = {
        // Snapshot registry + previous router under synchronous locks so
        // we don't hold any guards across the await on `build_router`.
        // Tauri requires command futures to be `Send`; neither guard is.
        let mut router_guard = state.mcp_router.lock().await;
        let previous = router_guard.take();
        drop(router_guard);

        let state_tools = {
            let reg = state.tool_registry.read().unwrap();
            let bare: Vec<String> = reg
                .state_owned_tools()
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            reg.to_llm_tools(&bare)
        };

        build_router(path.as_deref(), Some(&app), state_tools, previous)
            .await
            .map_err(|e| format!("Failed to rebuild MCP router: {e}"))?
    };

    // 4. Store the new router.
    *state.mcp_router.lock().await = Some(new_router);

    // 5. Notify the UI.
    app.emit(
        "mcp-router-rebuilt",
        serde_json::json!({ "spec_tools_enabled": path.is_some() }),
    )
    .ok();

    Ok(())
}