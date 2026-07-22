use serde::{Deserialize, Serialize};

/// Parsed YAML frontmatter of a state markdown file
#[derive(Debug, Clone, Deserialize)]
pub struct StateFrontmatter {
    pub name: String,
    pub description: String,
    pub valid_transitions: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Full parsed state definition (frontmatter + system prompt body)
#[derive(Debug, Clone)]
pub struct StateDefinition {
    pub frontmatter: StateFrontmatter,
    pub system_prompt: String,
}

/// Parsed YAML frontmatter of a tool markdown file
#[derive(Debug, Clone, Deserialize)]
pub struct ToolFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ToolParameter>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Full parsed tool definition (frontmatter + extended description)
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub frontmatter: ToolFrontmatter,
    pub extended_description: String,
}

/// Parsed YAML frontmatter of a card markdown file
#[derive(Debug, Clone, Deserialize)]
pub struct CardFrontmatter {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub relevant_states: Vec<String>,
}

/// Full parsed card definition.
/// generation_instructions: prose body (without the ```css block) — prompt for the orchestrator
/// css: extracted from the ```css fenced block at the end of the file body
#[derive(Debug, Clone)]
pub struct CardDefinition {
    pub frontmatter: CardFrontmatter,
    pub generation_instructions: String,
    pub css: Option<String>,
}

impl ToolDefinition {
    /// Convert parameters to OpenAI-compatible JSON Schema object
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.frontmatter.parameters {
            properties.insert(
                param.name.clone(),
                serde_json::json!({
                    "type": param.param_type,
                    "description": param.description,
                }),
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    }
}
