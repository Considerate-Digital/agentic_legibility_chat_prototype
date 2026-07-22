use tauri::{AppHandle, Emitter};

use crate::state_machine::loader::{
    load_card_registry, load_state_registry, load_tool_registry, parse_card_file,
    parse_state_file, parse_tool_file,
};
use crate::ManagedState;

#[derive(serde::Serialize)]
pub struct PlaygroundFiles {
    pub states: Vec<String>,
    pub tools: Vec<String>,
    pub cards: Vec<String>,
}

#[tauri::command]
pub fn list_playground_files(state: tauri::State<'_, ManagedState>) -> Result<PlaygroundFiles, String> {
    let dirs = state.playground_dirs.read().unwrap().clone();
    let states = list_md_files(&dirs.states)
        .map_err(|e| e.to_string())?;
    let tools = list_md_files(&dirs.tools.join("state"))
        .map_err(|e| e.to_string())?;
    let cards = list_md_files(&dirs.cards)
        .map_err(|e| e.to_string())?;
    Ok(PlaygroundFiles { states, tools, cards })
}

#[tauri::command]
pub fn read_playground_file(
    kind: String,
    filename: String,
    state: tauri::State<'_, ManagedState>,
) -> Result<String, String> {
    let path = resolve_path(&state, &kind, &filename).map_err(|e| e.to_string())?;
    std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {}", filename, e))
}

#[tauri::command]
pub fn write_playground_file(
    kind: String,
    filename: String,
    content: String,
    state: tauri::State<'_, ManagedState>,
    app: AppHandle,
) -> Result<(), String> {
    let path = resolve_path(&state, &kind, &filename).map_err(|e| e.to_string())?;

    // Validate frontmatter before writing to disk
    match kind.as_str() {
        "state" => parse_state_file(&content)
            .map(|_| ())
            .map_err(|e| format!("invalid state file: {}", e))?,
        "tool" => parse_tool_file(&content)
            .map(|_| ())
            .map_err(|e| format!("invalid tool file: {}", e))?,
        "card" => parse_card_file(&content)
            .map(|_| ())
            .map_err(|e| format!("invalid card file: {}", e))?,
        _ => return Err(format!("unknown kind '{}'", kind)),
    };

    std::fs::write(&path, content)
        .map_err(|e| format!("failed to write {}: {}", filename, e))?;

    reload_registries(&state, &app)
}

#[tauri::command]
pub fn delete_playground_file(
    kind: String,
    filename: String,
    state: tauri::State<'_, ManagedState>,
    app: AppHandle,
) -> Result<(), String> {
    let path = resolve_path(&state, &kind, &filename).map_err(|e| e.to_string())?;

    std::fs::remove_file(&path)
        .map_err(|e| format!("failed to delete {}: {}", filename, e))?;

    // If the deleted file was the active state, fall back to the first available state
    if kind == "state" {
        let deleted_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let current = state.current_state.read().unwrap().clone();
        if current == deleted_name {
            // Will be corrected after reload — just set to empty string for now
            // reload_registries will pick a valid fallback
        }
    }

    reload_registries(&state, &app)
}

// ── Helpers ───────────────────────────────────────────────────────────────

pub(crate) fn reload_registries(state: &ManagedState, app: &AppHandle) -> Result<(), String> {
    let dirs = state.playground_dirs.read().unwrap().clone();
    let new_state_reg = load_state_registry(&dirs.states)
        .map_err(|e| format!("reload failed: {}", e))?;
    let new_tool_reg = load_tool_registry(&dirs.tools)
        .map_err(|e| format!("reload failed: {}", e))?;
    let new_card_reg = load_card_registry(&dirs.cards)
        .map_err(|e| format!("reload failed: {}", e))?;

    // If current state no longer exists in the new registry, fall back to first available
    {
        let current = state.current_state.read().unwrap().clone();
        if new_state_reg.get(&current).is_none() {
            let fallback = new_state_reg
                .all_summaries()
                .into_iter()
                .next()
                .map(|s| s.name)
                .unwrap_or_else(|| "Idle".to_string());
            *state.current_state.write().unwrap() = fallback;
        }
    }

    *state.state_registry.write().unwrap() = new_state_reg;
    *state.tool_registry.write().unwrap() = new_tool_reg;
    *state.card_registry.write().unwrap() = new_card_reg;

    let summaries = state.state_registry.read().unwrap().all_summaries();
    app.emit("playground-reloaded", summaries).ok();

    // Push updated card CSS immediately so existing cards re-render without a new message
    let card_css: Vec<serde_json::Value> = state
        .card_registry
        .read()
        .unwrap()
        .all_css()
        .into_iter()
        .map(|(name, css)| serde_json::json!({ "name": name, "css": css }))
        .collect();
    app.emit("card-css-reloaded", card_css).ok();

    Ok(())
}

fn resolve_path(
    state: &ManagedState,
    kind: &str,
    filename: &str,
) -> anyhow::Result<std::path::PathBuf> {
    // Reject any path traversal attempts
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        anyhow::bail!("invalid filename '{}'", filename);
    }
    if !filename.ends_with(".md") {
        anyhow::bail!("filename must end with .md");
    }
    let dirs = state.playground_dirs.read().unwrap();
    // Tools live under <tools_dir>/state/ — the only server whose tools
    // are editable from the playground today.
    let path = match kind {
        "state" => dirs.states.join(filename),
        "tool" => dirs.tools.join("state").join(filename),
        "card" => dirs.cards.join(filename),
        _ => anyhow::bail!("unknown kind '{}'; expected 'state', 'tool', or 'card'", kind),
    };
    Ok(path)
}

fn list_md_files(dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}
