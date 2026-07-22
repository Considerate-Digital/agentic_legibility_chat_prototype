use std::path::PathBuf;
use std::sync::RwLock;

use config::AppConfig;
use llm::types::LLMToolDef;
use mcp::{
    namespaced_tools, router::McpServerHandle, McpClientEnum, McpRouter, LegibilityChatClient,
    ToolSource,
};
use tauri::{Emitter, Manager};
use state_machine::{
    loader::{load_card_registry, load_state_registry, load_tool_registry, seed_cards_dir, seed_playground_dirs},
    registry::{CardRegistry, StateRegistry, ToolRegistry},
};

mod commands;
mod config;
mod llm;
pub mod mcp;
mod state_machine;

#[derive(Clone)]
pub struct PlaygroundDirs {
    pub states: PathBuf,
    pub tools: PathBuf,
    pub cards: PathBuf,
}

pub struct ManagedState {
    pub current_state: RwLock<String>,
    pub state_registry: RwLock<StateRegistry>,
    pub tool_registry: RwLock<ToolRegistry>,
    pub card_registry: RwLock<CardRegistry>,
    pub config: RwLock<AppConfig>,
    pub conversation: RwLock<Vec<llm::types::ChatMessage>>,
    /// Lazily initialised on first send_message call / rebuilt whenever
    /// `live_resources_dir` changes. Holds a single MCP server, "state"
    /// (the `legibility-chat-mcp` stdio sidecar), which natively exposes the
    /// spec-lookup tools whenever `LIVE_RESOURCES_DIR` is set. Kept as a
    /// `McpRouter` (rather than inlining a single client) so a future
    /// third-party MCP server can be added without reshaping callers.
    pub mcp_router: tokio::sync::Mutex<Option<McpRouter>>,
    pub playground_dirs: RwLock<PlaygroundDirs>,
    /// Held while waiting for the user to submit a `ui_input` form.
    /// The sender is set by `dispatch_tool` and resolved by `submit_ui_input`.
    pub pending_ui_input: tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>,
}

// Safety: all fields are Send + Sync via RwLock / Mutex
unsafe impl Send for ManagedState {}
unsafe impl Sync for ManagedState {}

/// Build (or rebuild) the MCP router for the given configuration.
///
/// Spawns a single sidecar, `legibility-chat-mcp`, always passing the current
/// `live_resources_dir` via the `LIVE_RESOURCES_DIR` env var — the sidecar
/// natively registers the spec-lookup tools (`list_endpoints`, `get_service`,
/// …) whenever that's set, and omits them otherwise. Because the env var can
/// only take effect at process start, `build_router` always tears down any
/// previous "state" server and respawns a fresh one, rather than reusing an
/// existing process (mirrors how the old two-sidecar design always
/// tore down and respawned the separate `legibility-mcp` HTTP sidecar).
///
/// `app_handle` must be `Some` — spawning/respawning the sidecar always goes
/// through the shell plugin, which needs a Tauri `AppHandle`.
///
/// `state_tools` is the pre-built list of LLMToolDefs for the state server,
/// computed at the call site from `ToolRegistry` so the registry lock is
/// released before this async function is invoked. (Tauri requires command
/// futures to be `Send`, and `RwLockReadGuard` is `!Send`.) After spawning,
/// we also call `tools_list()` on the fresh sidecar and merge in anything it
/// advertises that isn't already in `state_tools` — markdown-sourced defs
/// win on name conflicts, mirroring `merge_router_tools` in `commands/chat.rs`.
pub async fn build_router(
    live_resources_dir: Option<&str>,
    app_handle: Option<&tauri::AppHandle>,
    state_tools: Vec<LLMToolDef>,
    previous: Option<McpRouter>,
) -> anyhow::Result<McpRouter> {
    let mut servers: Vec<McpServerHandle> = match previous {
        Some(r) => r.into_servers(),
        None => Vec::new(),
    };

    // ── legibility-chat-mcp ─────────────────────────────────────────────
    // Always shut down and drop any existing "state" handle so we can
    // respawn cleanly with the current `live_resources_dir` value.
    if let Some(existing) = servers.iter().find(|s| s.name == "state") {
        if let McpClientEnum::Stdio(client) = &existing.client {
            client.shutdown().await.ok();
        }
    }
    servers.retain(|s| s.name != "state");

    let handle = app_handle
        .ok_or_else(|| anyhow::anyhow!("legibility-chat-mcp requires a Tauri AppHandle to (re)spawn"))?;
    let client = LegibilityChatClient::new(handle, live_resources_dir).await?;

    // `state_tools` was built at the call site from `ToolRegistry` and holds
    // bare-name LLMToolDefs (`change_state`, `ui_input`, …). Merge in the
    // sidecar's own `tools/list` (e.g. the spec tools gated on
    // `LIVE_RESOURCES_DIR`) without clobbering any markdown-defined tool of
    // the same name.
    let list = client.tools_list().await?;
    let fetched = namespaced_tools("state", &list);
    let mut tools = state_tools;
    let mut seen: std::collections::HashSet<String> =
        tools.iter().map(|t| t.function.name.clone()).collect();
    for t in fetched {
        if seen.insert(t.function.name.clone()) {
            tools.push(t);
        }
    }

    servers.push(McpServerHandle {
        name: "state".into(),
        client: McpClientEnum::Stdio(client),
        tools,
        source: ToolSource::Markdown,
    });

    Ok(McpRouter::from_servers(servers))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise tracing once per process. Honours RUST_LOG (e.g.
    // RUST_LOG=legibility_chat=debug for verbose tool/LLM logging).
    // Idempotent — guards against double init in test/example contexts.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();

    let config = AppConfig::load();

    let states_dir = config
        .states_override_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .expect("could not determine config dir")
                .join("legibility-chat")
                .join("states")
        });

    let tools_dir = config
        .tools_override_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .expect("could not determine config dir")
                .join("legibility-chat")
                .join("tools")
        });

    let cards_dir = config
        .cards_override_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .expect("could not determine config dir")
                .join("legibility-chat")
                .join("cards")
        });

    // Registries are populated inside `.setup()` (below) so we have an
    // `AppHandle` for `path().resource_dir()` to resolve the bundled
    // `defaults/` resource. Until that completes, commands that read the
    // registries will see empty maps — fine, because `.setup()` runs
    // before any user command can fire.
    let initial_live_resources_dir = config.live_resources_dir.clone();

    let managed = ManagedState {
        current_state: RwLock::new(String::new()),
        state_registry: RwLock::new(StateRegistry::empty()),
        tool_registry: RwLock::new(ToolRegistry::empty()),
        card_registry: RwLock::new(CardRegistry::empty()),
        config: RwLock::new(config),
        conversation: RwLock::new(Vec::new()),
        mcp_router: tokio::sync::Mutex::new(None),
        pending_ui_input: tokio::sync::Mutex::new(None),
        playground_dirs: RwLock::new(PlaygroundDirs {
            states: states_dir,
            tools: tools_dir,
            cards: cards_dir,
        }),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(managed)
        .setup(move |app| {
            // ── Seed + load registries ─────────────────────────────────────
            // We need an `AppHandle` to resolve the bundled `defaults/`
            // resource dir, so this runs inside `.setup()` rather than in
            // `run()`. The ManagedState was constructed with empty
            // registries; we populate them now before any command can fire.
            let handle = app.handle().clone();

            // `resource_dir()` returns the bundled resources path in
            // production builds, but during `tauri dev` the bundler doesn't
            // run, so `bundle.resources` from tauri.conf.json is ignored
            // and resource_dir() just points at the exe's directory
            // (target/debug/). Fall back to the compile-time source-tree
            // path so dev builds find defaults/*.md without any extra
            // setup.
            let defaults_root = {
                let bundled = handle
                    .path()
                    .resource_dir()
                    .ok()
                    .map(|d| d.join("defaults"));
                let source_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/defaults");
                match bundled {
                    Some(b) if b.exists() => b,
                    _ => source_tree,
                }
            };
            let playground = handle.state::<ManagedState>().playground_dirs.read().unwrap().clone();

            // Seed (non-destructive — only writes when user dirs are empty)
            // then load. If loading fails we log and continue with empty
            // registries rather than panicking, so a broken defaults/
            // bundle doesn't kill the whole app.
            if let Err(e) = seed_playground_dirs(
                &defaults_root,
                &playground.states,
                &playground.tools,
            ) {
                eprintln!("loader: failed to seed playground dirs: {e:#}");
            }
            if let Err(e) = seed_cards_dir(&defaults_root, &playground.cards) {
                eprintln!("loader: failed to seed cards dir: {e:#}");
            }

            let state_reg = load_state_registry(&playground.states)
                .unwrap_or_else(|e| {
                    eprintln!("loader: state registry empty: {e:#}");
                    StateRegistry::empty()
                });
            let tool_reg = load_tool_registry(&playground.tools)
                .unwrap_or_else(|e| {
                    eprintln!("loader: tool registry empty: {e:#}");
                    ToolRegistry::empty()
                });
            let card_reg = load_card_registry(&playground.cards)
                .unwrap_or_else(|e| {
                    eprintln!("loader: card registry empty: {e:#}");
                    CardRegistry::empty()
                });

            // Pick a starting state from the loaded registry. Sort
            // alphabetically so the choice is deterministic across launches
            // (the user can change it via the State Selector). Empty string
            // is fine if no states were found — the UI handles that.
            let initial_state_name = {
                let mut summaries = state_reg.all_summaries();
                summaries.sort_by(|a, b| a.name.cmp(&b.name));
                summaries
                    .into_iter()
                    .next()
                    .map(|s| s.name)
                    .unwrap_or_default()
            };

            {
                let state = handle.state::<ManagedState>();
                *state.state_registry.write().unwrap() = state_reg;
                *state.tool_registry.write().unwrap() = tool_reg;
                *state.card_registry.write().unwrap() = card_reg;
                *state.current_state.write().unwrap() = initial_state_name;
            }

            // Build the MCP router in the background so first message isn't slow.
            let handle = app.handle().clone();
            let live_resources_dir = initial_live_resources_dir.clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<ManagedState>();
                // Snapshot the registry synchronously so the RwLockReadGuard
                // is dropped before the await on `build_router`. Tauri requires
                // command futures to be `Send`; the guard is `!Send`.
                let state_tools = {
                    let reg = state.tool_registry.read().unwrap();
                    let bare: Vec<String> = reg
                        .state_owned_tools()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    reg.to_llm_tools(&bare)
                };
                let result =
                    build_router(live_resources_dir.as_deref(), Some(&handle), state_tools, None).await;
                match result {
                    Ok(router) => {
                        *state.mcp_router.lock().await = Some(router);
                        // Notify the UI so the wizard / top-bar indicator reflect
                        // spec_tools_ready without the user having to click
                        // "Run health check" or change the config.
                        handle.emit(
                            "mcp-router-rebuilt",
                            serde_json::json!({ "spec_tools_enabled": live_resources_dir.is_some() }),
                        ).ok();
                    }
                    Err(e) => {
                        // Print the full anyhow error chain so the actual
                        // cause isn't hidden inside a `Context` wrapper.
                        eprintln!("Warning: MCP router failed to build");
                        for cause in e.chain() {
                            eprintln!("  caused by: {cause}");
                        }
                        eprintln!("  full: {e:#}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::send_message,
            commands::chat::clear_conversation,
            commands::chat::submit_ui_input,
            commands::state::get_state,
            commands::state::get_all_states,
            commands::state::set_state,
            commands::config::get_config,
            commands::config::set_config,
            commands::files::list_playground_files,
            commands::files::read_playground_file,
            commands::files::write_playground_file,
            commands::files::delete_playground_file,
            commands::live_resources::pick_live_resources_dir,
            commands::live_resources::set_live_resources_dir,
            commands::live_resources::scan_live_resources_dir,
            commands::wizard::test_llm_connection,
            commands::wizard::get_setup_status,
            commands::wizard::reset_to_defaults,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
