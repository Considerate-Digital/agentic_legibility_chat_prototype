use serde::{Deserialize, Serialize};

/// Full OpenAI-compatible chat completion request
#[derive(Debug, Serialize)]
pub struct LLMRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<LLMToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Tool definition in OpenAI format, sent in the LLM request
#[derive(Debug, Clone, Serialize)]
pub struct LLMToolDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: LLMFunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMFunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ── SSE streaming types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Delta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<PartialToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct PartialToolCall {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<PartialFunctionCall>,
}

#[derive(Debug, Deserialize)]
pub struct PartialFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Non-streaming response shapes (for the state-evaluation call)
#[derive(Debug, Deserialize)]
pub struct CompletionResponse {
    pub choices: Vec<CompletionChoice>,
}

#[derive(Debug, Deserialize)]
pub struct CompletionChoice {
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Result of a completed streaming LLM call
#[derive(Debug)]
pub struct CompletionOutcome {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: String,
}

/// Accumulator for a tool call being built across SSE chunks
#[derive(Debug, Default)]
pub struct ToolCallAccum {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCallAccum {
    pub fn into_tool_call(self) -> ToolCall {
        ToolCall {
            id: self.id,
            call_type: "function".into(),
            function: FunctionCall {
                name: self.name,
                arguments: self.arguments,
            },
        }
    }
}
