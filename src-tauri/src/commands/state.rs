use tauri::{AppHandle, Emitter};

use crate::state_machine::registry::{StateSummary, StateView};
use crate::ManagedState;

#[tauri::command]
pub fn get_state(state: tauri::State<'_, ManagedState>) -> Result<StateView, String> {
    let current = state.current_state.read().unwrap().clone();
    state_to_view(&state, &current)
        .ok_or_else(|| format!("state '{}' not found in registry", current))
}

#[tauri::command]
pub fn get_all_states(state: tauri::State<'_, ManagedState>) -> Result<Vec<StateSummary>, String> {
    Ok(state.state_registry.read().unwrap().all_summaries())
}

#[tauri::command]
pub fn set_state(
    target: String,
    state: tauri::State<'_, ManagedState>,
    app: AppHandle,
) -> Result<StateView, String> {
    if state.state_registry.read().unwrap().get(&target).is_none() {
        return Err(format!("'{}' is not a recognised state", target));
    }

    *state.current_state.write().unwrap() = target.clone();

    let view = state_to_view(&state, &target)
        .ok_or_else(|| format!("state '{}' has no definition", target))?;

    app.emit("state-changed", &view).ok();
    Ok(view)
}

pub fn state_to_view(state: &ManagedState, name: &str) -> Option<StateView> {
    let registry = state.state_registry.read().unwrap();
    let def = registry.get(name)?;
    Some(StateView {
        name: def.frontmatter.name.clone(),
        description: def.frontmatter.description.clone(),
        valid_transitions: def.frontmatter.valid_transitions.clone(),
        tools: registry.tools_for_state(name),
        system_prompt: def.system_prompt.clone(),
    })
}
