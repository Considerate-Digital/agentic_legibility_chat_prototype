use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            model: "gpt-4o".into(),
        }
    }
}

/// Optional separate config for the state-analyser call.
/// Unset fields fall back to the main provider's values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyserConfig {
    /// Model to use for state analysis — should be cheap and fast, e.g. gpt-4o-mini
    pub model: String,
    /// Override base URL (uses main provider's if absent)
    pub base_url: Option<String>,
    /// Override API key (uses main provider's if absent)
    pub api_key: Option<String>,
}

impl AnalyserConfig {
    /// Resolve into a full ProviderConfig by filling gaps from the main provider.
    pub fn resolve(&self, main: &ProviderConfig) -> ProviderConfig {
        ProviderConfig {
            base_url: self.base_url.clone().unwrap_or_else(|| main.base_url.clone()),
            api_key: self.api_key.clone().unwrap_or_else(|| main.api_key.clone()),
            model: self.model.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: ProviderConfig,
    /// Separate model for state evaluation. Falls back to main provider if absent.
    pub analyser: Option<AnalyserConfig>,
    /// Optional path to override state markdown files at runtime
    pub states_override_dir: Option<String>,
    /// Optional path to override tool markdown files at runtime
    pub tools_override_dir: Option<String>,
    /// Optional path to override card markdown files at runtime
    pub cards_override_dir: Option<String>,
    /// Live resources directory passed to `legibility-chat-mcp` via
    /// `LIVE_RESOURCES_DIR`. When `Some`, the sidecar natively registers the
    /// spec-lookup tools (`list_endpoints`, `get_service`, …); when `None`,
    /// only the base state tools are available.
    /// File-picker target — pick a directory containing `endpoints/`, `services/`,
    /// and `plans/` subdirs (the layout the spec-lookup tools expect).
    #[serde(default)]
    pub live_resources_dir: Option<String>,
    /// Whether `send_message` runs the card-selector LLM call, which may
    /// rewrite the assistant's response to fit a UI card's format. Off by
    /// default means the raw streamed response is shown verbatim.
    #[serde(default = "default_cards_enabled")]
    pub cards_enabled: bool,
}

fn default_cards_enabled() -> bool {
    false
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            analyser: None,
            states_override_dir: None,
            tools_override_dir: None,
            cards_override_dir: None,
            live_resources_dir: None,
            cards_enabled: default_cards_enabled(),
        }
    }
}

impl AppConfig {
    /// Returns the effective ProviderConfig for the state analyser.
    pub fn analyser_provider(&self) -> ProviderConfig {
        match &self.analyser {
            Some(a) => a.resolve(&self.provider),
            None => self.provider.clone(),
        }
    }

    pub fn load() -> Self {
        let path = config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("legibility-chat")
        .join("config.json")
}
