use crate::config::AppConfig;
use crate::ManagedState;

#[tauri::command]
pub fn get_config(state: tauri::State<'_, ManagedState>) -> Result<AppConfig, String> {
    Ok(state.config.read().unwrap().clone())
}

#[tauri::command]
pub async fn set_config(
    config: AppConfig,
    state: tauri::State<'_, ManagedState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Detect whether any playground directory changed before we overwrite the config.
    let dirs_changed = {
        let old = state.config.read().unwrap();
        old.states_override_dir != config.states_override_dir
            || old.tools_override_dir != config.tools_override_dir
            || old.cards_override_dir != config.cards_override_dir
    };

    config.save().map_err(|e| e.to_string())?;
    *state.config.write().unwrap() = config.clone();

    if dirs_changed {
        let default_base = dirs::config_dir()
            .expect("could not determine config dir")
            .join("legibility-chat");
        let new_dirs = crate::PlaygroundDirs {
            states: config.states_override_dir.as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| default_base.join("states")),
            tools: config.tools_override_dir.as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| default_base.join("tools")),
            cards: config.cards_override_dir.as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| default_base.join("cards")),
        };
        *state.playground_dirs.write().unwrap() = new_dirs;
        crate::commands::files::reload_registries(&state, &app)?;
    }

    Ok(())
}
