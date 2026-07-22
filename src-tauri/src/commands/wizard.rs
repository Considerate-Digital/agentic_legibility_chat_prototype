//! Commands supporting the first-launch setup wizard.
//!
//! - `test_llm_connection` — POSTs to `{base_url}/models` with Bearer auth
//!   to confirm the user's API key + base URL actually work. Used by the
//!   wizard's Step 2 to give instant feedback.
//! - `get_setup_status` — snapshots what's configured and which MCP servers
//!   are running, so the wizard / top-bar Setup button can decide what to
//!   show.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri::http::StatusCode;

use crate::mcp::is_spec_tool;
use crate::state_machine::loader::{
    load_card_registry, load_state_registry, load_tool_registry,
    reset_to_defaults as loader_reset_to_defaults,
};
use crate::ManagedState;

/// Snapshot of what's configured and what's healthy, returned by
/// `get_setup_status`.
///
/// Used by the wizard's first-run detection (any of `has_provider` /
/// `has_live_resources_dir` is false → show wizard) and by the top-bar
/// "Setup" button's incomplete-dot indicator.
#[derive(Debug, Clone, Serialize)]
pub struct SetupStatus {
    /// True when `provider.api_key` is non-empty AND `provider.model` is set.
    pub has_provider: bool,
    /// True when `analyser` is configured with a non-empty model.
    pub has_analyser: bool,
    /// True when `live_resources_dir` is set in config.
    pub has_live_resources_dir: bool,
    /// True when the "state" server's advertised tool list (from its
    /// `tools/list`) contains at least one known spec-lookup tool name.
    /// This is a stronger signal than "is a server registered" — it
    /// reflects the `legibility-chat-mcp` sidecar having successfully loaded the
    /// `live_resources_dir` and registered the gated tools, not just a
    /// process existing. (False during startup before the background
    /// build_router completes, or when `live_resources_dir` is unset.)
    pub spec_tools_ready: bool,
}

/// Read-only snapshot of config + router state. Called by the wizard on
/// mount and after `mcp-router-rebuilt` events so the UI can re-evaluate
/// the "is setup complete?" question without watching individual fields.
#[tauri::command]
pub async fn get_setup_status(
    state: tauri::State<'_, ManagedState>,
) -> Result<SetupStatus, String> {
    let (has_provider, has_analyser, has_live_resources_dir) = {
        let cfg = state.config.read().unwrap();
        let has_provider =
            !cfg.provider.api_key.is_empty() && !cfg.provider.model.is_empty();
        let has_analyser = cfg
            .analyser
            .as_ref()
            .map(|a| !a.model.is_empty())
            .unwrap_or(false);
        let has_live_resources_dir = cfg.live_resources_dir.is_some();
        (has_provider, has_analyser, has_live_resources_dir)
    };

    // `try_lock` because the router may be held by the background
    // `build_router` task on startup. If we can't grab it, treat the spec
    // tools as "not ready yet" — the next mount / event tick will
    // re-evaluate.
    let spec_tools_ready = match state.mcp_router.try_lock() {
        Ok(guard) => guard
            .as_ref()
            .map(|r| r.tools_for("state").iter().any(|n| is_spec_tool(n)))
            .unwrap_or(false),
        Err(_) => false,
    };

    Ok(SetupStatus {
        has_provider,
        has_analyser,
        has_live_resources_dir,
        spec_tools_ready,
    })
}

/// Test the user's LLM endpoint by POSTing `{base_url}/models` with the
/// supplied API key as Bearer auth.
///
/// Returns `Ok(())` on any 2xx response. On failure, returns a short
/// human-readable message that the wizard shows inline under the form.
///
/// We deliberately probe `/models` rather than `/chat/completions` because
/// it accepts no body and is cheap, and almost every OpenAI-compatible
/// endpoint (OpenAI, Anthropic's compat endpoint, OpenRouter, Ollama, etc.)
/// implements it. Anthropic's native endpoint does *not* expose `/models`,
/// so this command will fail for that case — users on Anthropic should use
/// the OpenAI-compatible base URL `https://api.anthropic.com/v1`.
#[tauri::command]
pub async fn test_llm_connection(
    base_url: String,
    api_key: String,
) -> Result<(), String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(human_status_error(status, resp).await)
            }
        }
        Err(e) if e.is_timeout() => {
            Err(format!("Timed out reaching {url} after 10s — check the URL"))
        }
        Err(e) if e.is_connect() => {
            Err(format!("Could not reach {url} — check the URL and your network"))
        }
        Err(e) => Err(format!("Request failed: {e}")),
    }
}

/// Map a non-success status to a short, actionable message. Reads the body
/// for a JSON `error.message` (OpenAI / OpenRouter shape) when present;
/// otherwise falls back to the status text.
async fn human_status_error(
    status: StatusCode,
    resp: reqwest::Response,
) -> String {
    let body = resp.text().await.unwrap_or_default();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return format!("HTTP {status}: {msg}");
        }
    }

    match status.as_u16() {
        401 => "Unauthorized (401) — check your API key".into(),
        403 => "Forbidden (403) — your key may not have access to this endpoint".into(),
        404 => "Not Found (404) — check your base URL ends in `/v1`".into(),
        429 => "Rate limited (429) — wait a moment and try again".into(),
        _ => format!("HTTP {status}: {}", status.canonical_reason().unwrap_or("error")),
    }
}

/// Reset the playground (states/tools/cards) to the bundled defaults.
///
/// Wipes the user-config dirs under `~/.config/legibility-chat/{states,tools,cards}/`
/// and re-copies the `.md` files from the bundled `defaults/` resource.
/// The user has opted into losing their edits — the Config panel prompts
/// for confirmation before calling this.
///
/// On success, emits `playground-reloaded` so the UI re-renders against
/// the fresh registries.
#[tauri::command]
pub async fn reset_to_defaults(
    app: AppHandle,
) -> Result<(), String> {
    // Same fallback as in lib.rs::run().setup(): prefer the bundled
    // resource (production), fall back to the compile-time source-tree
    // path (tauri dev, where the bundler doesn't run).
    let defaults_root = {
        let bundled = app
            .path()
            .resource_dir()
            .ok()
            .map(|d| d.join("defaults"));
        let source_tree = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/defaults");
        match bundled {
            Some(b) if b.exists() => b,
            _ => source_tree,
        }
    };

    if !defaults_root.exists() {
        return Err(format!(
            "defaults/ resource not found at {:?}; the bundled files are \
             missing from this build",
            defaults_root
        ));
    }

    let playground = {
        let state = app.state::<ManagedState>();
        let dirs = state.playground_dirs.read().unwrap().clone();
        dirs
    };

    // Wipe + re-copy. Both steps surface as one error if either fails.
    loader_reset_to_defaults(
        &defaults_root,
        &playground.states,
        &playground.tools,
        &playground.cards,
    )
    .map_err(|e| format!("reset failed: {e:#}"))?;

    // Reload the registries from the freshly-reset dirs and swap them into
    // ManagedState. If reload fails we restore the *previous* registries by
    // simply not overwriting — but in practice a successful reset implies
    // successful reload (the same files we just wrote).
    let state_reg = load_state_registry(&playground.states)
        .map_err(|e| format!("reload states failed: {e:#}"))?;
    let tool_reg = load_tool_registry(&playground.tools)
        .map_err(|e| format!("reload tools failed: {e:#}"))?;
    let card_reg = load_card_registry(&playground.cards)
        .map_err(|e| format!("reload cards failed: {e:#}"))?;

    let summaries;
    {
        let state = app.state::<ManagedState>();
        *state.state_registry.write().unwrap() = state_reg;
        *state.tool_registry.write().unwrap() = tool_reg;
        *state.card_registry.write().unwrap() = card_reg;
        // After reset, the previous current_state may not exist anymore.
        // Snap to the first available state (alphabetical) so the UI is
        // never pointing at a ghost.
        let state_reg = state.state_registry.read().unwrap();
        let mut all = state_reg.all_summaries();
        drop(state_reg);
        all.sort_by(|a, b| a.name.cmp(&b.name));
        let first = all.into_iter().next().map(|s| s.name).unwrap_or_default();
        *state.current_state.write().unwrap() = first.clone();

        summaries = state.state_registry.read().unwrap().all_summaries();
    }

    // Tell the UI to refresh — the same event the `reload_playground`
    // command emits, so the StateSelector / card panel both re-render.
    app.emit("playground-reloaded", summaries).ok();

    Ok(())
}
