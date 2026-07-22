use std::collections::HashMap;

use super::types::{CardDefinition, StateDefinition, ToolDefinition};
use crate::llm::types::{LLMFunctionDef, LLMToolDef};

pub struct StateRegistry {
    pub(super) states: HashMap<String, StateDefinition>,
}

pub struct ToolRegistry {
    pub(super) tools: HashMap<String, ToolDefinition>,
}

impl StateRegistry {
    /// Empty registry — used as a placeholder before `loader` populates
    /// the real one inside Tauri's `.setup()` callback (where the AppHandle
    /// is available for resolving the bundled `defaults/` resource dir).
    pub fn empty() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&StateDefinition> {
        self.states.get(name)
    }

    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        self.states
            .get(from)
            .map(|d| d.frontmatter.valid_transitions.iter().any(|t| t == to))
            .unwrap_or(false)
    }

    /// Returns tool names for the given state, always including `change_state`
/// (the in-process state-transition pseudo-tool).
    pub fn tools_for_state(&self, state: &str) -> Vec<String> {
        let mut tools: Vec<String> = self
            .states
            .get(state)
            .map(|d| d.frontmatter.tools.clone())
            .unwrap_or_default();

        if !tools.contains(&"change_state".to_string()) {
            tools.push("change_state".to_string());
        }
        tools
    }

    pub fn all_summaries(&self) -> Vec<StateSummary> {
        let mut summaries: Vec<StateSummary> = self
            .states
            .values()
            .map(|d| StateSummary {
                name: d.frontmatter.name.clone(),
                description: d.frontmatter.description.clone(),
            })
            .collect();
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }
}

impl ToolRegistry {
    /// Empty registry — placeholder before `loader` populates it. See
    /// `StateRegistry::empty` for why this exists.
    pub fn empty() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Returns LLM-formatted tool definitions for the given bare tool names.
    ///
    /// State `.md` files and the `change_state` injection both refer to
    /// tools by their bare names (`get_service`, `change_state`), so
    /// this function accepts bare names as input. Internally the registry
    /// is keyed by `<server>_<tool>` so two servers can own a tool with
    /// the same bare name; we look up by bare name and emit `function.name`
    /// as the bare name verbatim — that way the LLM sees and calls
    /// `get_service`, not `state_get_service`.
    pub fn to_llm_tools(&self, names: &[String]) -> Vec<LLMToolDef> {
        names
            .iter()
            .filter_map(|bare_name| {
                let def = self.lookup_by_bare_name(bare_name)?;
                Some(LLMToolDef {
                    def_type: "function".to_string(),
                    function: LLMFunctionDef {
                        name: bare_name.clone(),
                        description: def.frontmatter.description.clone(),
                        parameters: def.to_json_schema(),
                    },
                })
            })
            .collect()
    }

    /// Find a tool by its bare name (the part after `<server>_`). Returns
    /// the first match; if two servers both expose the same bare name, the
    /// caller should disambiguate via the router.
    fn lookup_by_bare_name(&self, bare_name: &str) -> Option<&ToolDefinition> {
        self.tools.values().find(|def| def.frontmatter.name == bare_name)
    }

    /// Returns the bare names of every tool owned by the `state` server.
    /// Filters by the registry key prefix (`state_`) so the call works
    /// correctly even if tools from other servers are ever loaded into the
    /// same registry (currently only `state_` keys exist).
    pub fn state_owned_tools(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .tools
            .iter()
            .filter(|(key, _)| key.starts_with("state_"))
            .map(|(_, def)| def.frontmatter.name.as_str())
            .collect();
        out.sort_unstable();
        out
    }
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct StateSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct StateView {
    pub name: String,
    pub description: String,
    pub valid_transitions: Vec<String>,
    pub tools: Vec<String>,
    pub system_prompt: String,
}

pub struct CardRegistry {
    pub(super) cards: HashMap<String, CardDefinition>,
}

impl CardRegistry {
    /// Empty registry — placeholder before `loader` populates it. See
    /// `StateRegistry::empty` for why this exists.
    pub fn empty() -> Self {
        Self {
            cards: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&CardDefinition> {
        self.cards.get(name)
    }

    pub fn all_summaries(&self) -> Vec<CardSummary> {
        let mut summaries: Vec<CardSummary> = self
            .cards
            .values()
            .map(|d| CardSummary {
                name: d.frontmatter.name.clone(),
                description: d.frontmatter.description.clone(),
                generation_instructions: d.generation_instructions.clone(),
            })
            .collect();
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Returns (name, css) pairs for all cards that have CSS defined.
    pub fn all_css(&self) -> Vec<(String, String)> {
        self.cards
            .values()
            .filter_map(|d| {
                d.css
                    .as_ref()
                    .map(|css| (d.frontmatter.name.clone(), css.clone()))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct CardSummary {
    pub name: String,
    pub description: String,
    pub generation_instructions: String,
}
