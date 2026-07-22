//! Anthropic-native provider — `POST {root}/v1/messages`.
//!
//! Distinct from the OpenAI-compatible path because Anthropic uses:
//! - `x-api-key` auth header (not `Authorization: Bearer`)
//! - A separate top-level `system` field (not a `system` message in `messages`)
//! - Flat tool definitions (`{name, description, input_schema}` not the
//!   OpenAI `{type: "function", function: {…}}` nesting)
//! - A required `max_tokens` field
//! - Different SSE event types (`content_block_delta`, `message_delta`, …)
//!
//! The body and SSE shapes are translated to/from our internal
//! `LLMRequest` / `CompletionOutcome` so the rest of the codebase doesn't
//! need to know which provider is in use.

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::config::ProviderConfig;

use super::stream::RawEvent;
use super::types::{ChatMessage, CompletionOutcome, LLMRequest, ToolCall, ToolCallAccum};

#[derive(Debug, serde::Serialize, Clone)]
struct ChunkEvent {
    delta: String,
}

/// Default `max_tokens` for Anthropic requests — Anthropic requires the field
/// and our config UI doesn't expose it, so we pick a value that's large
/// enough for chat responses with tool calls without paying for tokens we
/// won't use.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// POST URL for Anthropic's native endpoint.
///
/// `base_url` should be the API root, e.g. `https://api.anthropic.com/v1`.
/// We accept several common input shapes for robustness:
/// - `https://api.anthropic.com/v1` → `…/v1/messages`
/// - `https://api.anthropic.com` → `…/v1/messages` (we add `/v1` for users
///   who paste the bare origin)
/// - `https://api.anthropic.com/v1/messages` → `…/v1/messages` (strip suffix)
pub fn endpoint_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let root = trimmed
        .strip_suffix("/messages")
        .unwrap_or(trimmed);

    // If the root already has /v1 in it, leave it; otherwise prepend /v1.
    let root = if root.contains("/v1") {
        root.to_string()
    } else {
        format!("{}/v1", root)
    };
    format!("{}/messages", root)
}

/// Build an HTTP client with Anthropic's required `anthropic-version` header.
fn build_client() -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "anthropic-version",
        "2023-06-01".parse().unwrap(),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("building HTTP client")
}

// ── Request body translation ─────────────────────────────────────────────

/// Per-request Anthropic body shape.
///
/// We use `serde_json::Value` for `messages` / `tools` / `tool_choice` /
/// `system` so we can produce exactly the shapes Anthropic expects without
/// modelling every variant in a typed enum. The translator below is the
/// single source of truth for those shapes.
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Translate an internal `LLMRequest` to an Anthropic-shaped body.
///
/// The translation is pure (no I/O) so it's easy to unit-test.
fn translate_request(request: &LLMRequest, stream: bool) -> AnthropicRequest {
    let (system, messages) = split_system(&request.messages);
    let messages = messages
        .into_iter()
        .map(translate_message)
        .collect();
    let tools = request.tools.as_ref().map(|defs| {
        defs.iter().map(translate_tool_def).collect()
    });
    let tool_choice = request.tool_choice.as_ref().map(translate_tool_choice);

    AnthropicRequest {
        model: request.model.clone(),
        messages,
        system,
        tools,
        tool_choice,
        max_tokens: DEFAULT_MAX_TOKENS,
        stream,
        temperature: request.temperature,
    }
}

/// Pull the leading `system` message out of the conversation and return it
/// alongside the remaining messages. Anthropic takes the system prompt as a
/// separate top-level field rather than as a message in the `messages` array.
///
/// Multiple system messages are joined with `\n\n` — unusual but harmless.
fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<ChatMessage>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut rest: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    for m in messages {
        if m.role == "system" {
            if let Some(c) = &m.content {
                system_parts.push(c.clone());
            }
        } else {
            rest.push(m.clone());
        }
    }
    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };
    (system, rest)
}

/// Translate one OpenAI-shaped `ChatMessage` to an Anthropic-shaped JSON
/// value for the `messages` array.
fn translate_message(message: ChatMessage) -> Value {
    match message.role.as_str() {
        // Tool results come back as user messages with a list of
        // `tool_result` content blocks (Anthropic requires user-role).
        "tool" => {
            let id = message.tool_call_id.unwrap_or_default();
            let content = message.content.unwrap_or_default();
            json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": content,
                }]
            })
        }
        // Assistant messages with tool_calls must be sent as a list of
        // content blocks (text + tool_use). Plain text-only assistant
        // messages stay as a plain string content.
        "assistant" => {
            let tool_calls = message.tool_calls.unwrap_or_default();
            if tool_calls.is_empty() {
                json!({
                    "role": "assistant",
                    "content": message.content.unwrap_or_default(),
                })
            } else {
                let mut blocks: Vec<Value> = Vec::new();
                if let Some(text) = message.content {
                    if !text.is_empty() {
                        blocks.push(json!({"type": "text", "text": text}));
                    }
                }
                for tc in tool_calls {
                    // Anthropic `input` must be a JSON object; the OpenAI
                    // `arguments` field is a JSON-encoded string we have to
                    // parse. If it fails, fall back to an empty object so
                    // we never silently corrupt the call.
                    let input: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or_else(|_| json!({}));
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.function.name,
                        "input": input,
                    }));
                }
                json!({
                    "role": "assistant",
                    "content": blocks,
                })
            }
        }
        // User / other roles pass through as plain `{role, content}`.
        _ => json!({
            "role": message.role,
            "content": message.content.unwrap_or_default(),
        }),
    }
}

/// Translate one OpenAI-shaped tool definition to Anthropic's flat shape.
fn translate_tool_def(def: &super::types::LLMToolDef) -> Value {
    json!({
        "name": def.function.name,
        "description": def.function.description,
        "input_schema": def.function.parameters,
    })
}

/// Translate our internal `tool_choice` JSON to Anthropic's shape.
///
/// - OpenAI `{"type":"function","function":{"name":"X"}}` →
///   Anthropic `{"type":"tool","name":"X"}`
/// - OpenAI `{"type":"auto"}` / `{"type":"any"}` → pass through
/// - Anything else → pass through unchanged
fn translate_tool_choice(choice: &Value) -> Value {
    // The structured-function form we use elsewhere in the app.
    if choice.get("type") == Some(&Value::String("function".into())) {
        if let Some(func) = choice.get("function") {
            if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                return json!({"type": "tool", "name": name});
            }
        }
    }
    // Auto / any / etc. — pass through. Anthropic accepts the same strings.
    choice.clone()
}

// ── HTTP calls ───────────────────────────────────────────────────────────

/// Streaming completion against Anthropic's `/v1/messages`.
pub async fn stream(
    config: &ProviderConfig,
    request: &LLMRequest,
    app: &AppHandle,
) -> Result<CompletionOutcome> {
    let client = build_client()?;
    let url = endpoint_url(&config.base_url);
    let body = translate_request(request, true);

    let response = client
        .post(&url)
        .header("x-api-key", &config.api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("sending Anthropic request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("LLM API error {} (POST {}): {}", status, url, body));
    }

    let mut byte_stream = response.bytes_stream();
    let mut buf = String::new();
    let mut content_buf = String::new();
    let mut tool_accum: HashMap<usize, ToolCallAccum> = HashMap::new();
    let mut finish_reason = String::from("stop");

    while let Some(chunk) = byte_stream.next().await {
        let bytes = chunk.context("reading stream chunk")?;
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(block_end) = buf.find("\n\n") {
            let block = buf[..block_end].to_string();
            buf = buf[block_end + 2..].to_string();

            for event in parse_sse_block(&block) {
                apply_event(event, &mut content_buf, &mut tool_accum, &mut finish_reason, app);
            }
        }
    }

    let tool_calls = if tool_accum.is_empty() {
        None
    } else {
        let mut calls: Vec<_> = tool_accum.into_values().collect();
        calls.sort_by(|a, b| a.id.cmp(&b.id));
        Some(calls.into_iter().map(|a| a.into_tool_call()).collect())
    };
    let content = if content_buf.is_empty() { None } else { Some(content_buf) };

    Ok(CompletionOutcome { content, tool_calls, finish_reason })
}

/// Non-streaming completion against Anthropic's `/v1/messages`.
pub async fn complete(
    config: &ProviderConfig,
    request: &LLMRequest,
) -> Result<CompletionOutcome> {
    let client = build_client()?;
    let url = endpoint_url(&config.base_url);
    let body = translate_request(request, false);

    let response = client
        .post(&url)
        .header("x-api-key", &config.api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("sending Anthropic request (non-streaming)")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("LLM API error {} (POST {}): {}", status, url, body));
    }

    let resp: AnthropicResponse = response.json().await.context("parsing Anthropic response")?;
    Ok(translate_response(resp))
}

// ── Response translation ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    // Other content block types (image, document, etc.) are rare for our
    // use case; we silently skip them.
    #[serde(other)]
    Other,
}

fn translate_response(resp: AnthropicResponse) -> CompletionOutcome {
    let mut text_buf = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in resp.content {
        match block {
            AnthropicContentBlock::Text { text } => text_buf.push_str(&text),
            AnthropicContentBlock::ToolUse { id, name, input } => {
                // Re-serialise the parsed input back to a JSON string, since
                // our internal ToolCall stores arguments as a string.
                let arguments = serde_json::to_string(&input).unwrap_or_default();
                tool_calls.push(ToolCall {
                    id,
                    call_type: "function".into(),
                    function: super::types::FunctionCall {
                        name,
                        arguments,
                    },
                });
            }
            AnthropicContentBlock::Other => {}
        }
    }

    CompletionOutcome {
        content: if text_buf.is_empty() { None } else { Some(text_buf) },
        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        finish_reason: resp.stop_reason.unwrap_or_else(|| "stop".into()),
    }
}

// ── SSE streaming ────────────────────────────────────────────────────────

/// Parse one SSE block from an Anthropic streaming response.
///
/// Anthropic's stream is a sequence of named events. Each block contains
/// `event: <type>` and `data: <json>` lines. The `event` name tells us how
/// to interpret the `data` payload, so we collect both before deciding.
pub fn parse_sse_block(block: &str) -> Vec<RawEvent> {
    let mut event_name: Option<String> = None;
    let mut data: Option<String> = None;
    for line in block.lines() {
        if let Some(name) = line.strip_prefix("event: ") {
            event_name = Some(name.trim().to_string());
        } else if let Some(payload) = line.strip_prefix("data: ") {
            data = Some(payload.to_string());
        }
    }

    let Some(event) = event_name else { return Vec::new() };
    let Some(data) = data else { return Vec::new() };

    match event.as_str() {
        "content_block_start" => parse_content_block_start(&data),
        "content_block_delta" => parse_content_block_delta(&data),
        "message_delta" => parse_message_delta(&data),
        "message_stop" => vec![RawEvent::Finish("stop".into())],
        // message_start, content_block_stop, ping — no-op for our purposes.
        _ => Vec::new(),
    }
}

fn parse_content_block_start(data: &str) -> Vec<RawEvent> {
    #[derive(Deserialize)]
    struct Payload {
        index: usize,
        content_block: Value,
    }
    let Ok(p) = serde_json::from_str::<Payload>(data) else { return Vec::new() };
    match p.content_block.get("type").and_then(|v| v.as_str()) {
        Some("text") => Vec::new(),  // text blocks carry nothing useful at start
        Some("tool_use") => {
            let id = p.content_block.get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = p.content_block.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            vec![RawEvent::ToolCall {
                index: p.index,
                id: Some(id),
                name: Some(name),
                arguments_fragment: None,
            }]
        }
        _ => Vec::new(),
    }
}

fn parse_content_block_delta(data: &str) -> Vec<RawEvent> {
    #[derive(Deserialize)]
    struct Payload {
        index: usize,
        delta: Value,
    }
    let Ok(p) = serde_json::from_str::<Payload>(data) else { return Vec::new() };
    match p.delta.get("type").and_then(|v| v.as_str()) {
        Some("text_delta") => {
            let text = p.delta.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() { Vec::new() } else { vec![RawEvent::Text(text)] }
        }
        Some("input_json_delta") => {
            let fragment = p.delta.get("partial_json")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if fragment.is_empty() {
                Vec::new()
            } else {
                vec![RawEvent::ToolCall {
                    index: p.index,
                    id: None,
                    name: None,
                    arguments_fragment: Some(fragment),
                }]
            }
        }
        _ => Vec::new(),
    }
}

fn parse_message_delta(data: &str) -> Vec<RawEvent> {
    #[derive(Deserialize)]
    struct Payload {
        delta: Value,
    }
    let Ok(p) = serde_json::from_str::<Payload>(data) else { return Vec::new() };
    match p.delta.get("stop_reason").and_then(|v| v.as_str()) {
        Some(reason) if !reason.is_empty() => vec![RawEvent::Finish(reason.to_string())],
        _ => Vec::new(),
    }
}

fn apply_event(
    event: RawEvent,
    content_buf: &mut String,
    tool_accum: &mut HashMap<usize, ToolCallAccum>,
    finish_reason: &mut String,
    app: &AppHandle,
) {
    match event {
        RawEvent::Text(t) => {
            content_buf.push_str(&t);
            let _ = app.emit("llm-chunk", ChunkEvent { delta: t });
        }
        RawEvent::ToolCall { index, id, name, arguments_fragment } => {
            let entry = tool_accum.entry(index).or_default();
            if let Some(id) = id { entry.id = id; }
            if let Some(name) = name { entry.name.push_str(&name); }
            if let Some(args) = arguments_fragment { entry.arguments.push_str(&args); }
        }
        RawEvent::Finish(reason) => {
            if !reason.is_empty() {
                *finish_reason = reason;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{ChatMessage, FunctionCall, LLMFunctionDef, LLMToolDef, ToolCall};

    // ── URL builder ─────────────────────────────────────────────────────

    #[test]
    fn url_from_api_root_with_v1() {
        assert_eq!(
            endpoint_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages",
        );
    }

    #[test]
    fn url_from_anthropic_root_appends_v1() {
        assert_eq!(
            endpoint_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages",
        );
    }

    #[test]
    fn url_strips_existing_messages_suffix() {
        assert_eq!(
            endpoint_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com/v1/messages",
        );
        assert_eq!(
            endpoint_url("https://api.anthropic.com/messages"),
            "https://api.anthropic.com/v1/messages",
        );
    }

    // ── Request translation ─────────────────────────────────────────────

    fn make_request(messages: Vec<ChatMessage>, tools: Vec<LLMToolDef>) -> LLMRequest {
        LLMRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            messages,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice: None,
            stream: false,
            temperature: Some(0.7),
        }
    }

    #[test]
    fn system_message_becomes_top_level_field() {
        let req = make_request(
            vec![
                ChatMessage::system("You are helpful."),
                ChatMessage::user("hi"),
            ],
            vec![],
        );
        let body = translate_request(&req, false);
        assert_eq!(body.system.as_deref(), Some("You are helpful."));
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0]["role"], "user");
    }

    #[test]
    fn multiple_system_messages_join_with_double_newline() {
        let req = make_request(
            vec![
                ChatMessage::system("Rule A."),
                ChatMessage::system("Rule B."),
                ChatMessage::user("hi"),
            ],
            vec![],
        );
        let body = translate_request(&req, false);
        assert_eq!(body.system.as_deref(), Some("Rule A.\n\nRule B."));
    }

    #[test]
    fn missing_system_field_when_no_system_message() {
        let req = make_request(vec![ChatMessage::user("hi")], vec![]);
        let body = translate_request(&req, false);
        assert!(body.system.is_none());
    }

    #[test]
    fn plain_user_message_passes_through() {
        let req = make_request(vec![ChatMessage::user("hi")], vec![]);
        let body = translate_request(&req, false);
        assert_eq!(body.messages[0]["role"], "user");
        assert_eq!(body.messages[0]["content"], "hi");
    }

    #[test]
    fn tool_result_becomes_user_message_with_tool_result_block() {
        let req = make_request(
            vec![ChatMessage::tool_result("toolu_abc", "sunny")],
            vec![],
        );
        let body = translate_request(&req, false);
        assert_eq!(body.messages[0]["role"], "user");
        let blocks = &body.messages[0]["content"];
        assert_eq!(blocks[0]["type"], "tool_result");
        assert_eq!(blocks[0]["tool_use_id"], "toolu_abc");
        assert_eq!(blocks[0]["content"], "sunny");
    }

    #[test]
    fn assistant_with_tool_calls_uses_content_blocks() {
        let req = make_request(
            vec![ChatMessage {
                role: "assistant".into(),
                content: Some("let me check".into()),
                tool_calls: Some(vec![ToolCall {
                    id: "toolu_1".into(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "get_weather".into(),
                        arguments: r#"{"location":"London"}"#.into(),
                    },
                }]),
                tool_call_id: None,
                name: None,
            }],
            vec![],
        );
        let body = translate_request(&req, false);
        let blocks = &body.messages[0]["content"];
        assert!(blocks.is_array());
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "let me check");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["id"], "toolu_1");
        assert_eq!(blocks[1]["name"], "get_weather");
        assert_eq!(blocks[1]["input"]["location"], "London");
    }

    #[test]
    fn assistant_with_only_text_passes_through_as_string() {
        let req = make_request(
            vec![ChatMessage {
                role: "assistant".into(),
                content: Some("ok".into()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            vec![],
        );
        let body = translate_request(&req, false);
        assert_eq!(body.messages[0]["content"], "ok");
    }

    #[test]
    fn tool_def_uses_flat_input_schema() {
        let req = make_request(
            vec![ChatMessage::user("hi")],
            vec![LLMToolDef {
                def_type: "function".into(),
                function: LLMFunctionDef {
                    name: "get_weather".into(),
                    description: "Look up weather".into(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"],
                    }),
                },
            }],
        );
        let body = translate_request(&req, false);
        let tools = body.tools.expect("tools should be set");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "get_weather");
        assert_eq!(tools[0]["description"], "Look up weather");
        assert!(tools[0].get("input_schema").is_some());
        assert!(tools[0].get("function").is_none(), "must not have function nesting");
        assert!(tools[0].get("type").is_none(), "must not have type field");
    }

    #[test]
    fn tool_choice_function_becomes_tool_name() {
        let req = LLMRequest {
            model: "claude".into(),
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            tool_choice: Some(json!({
                "type": "function",
                "function": {"name": "change_state"}
            })),
            stream: false,
            temperature: None,
        };
        let body = translate_request(&req, false);
        assert_eq!(body.tool_choice.unwrap(), json!({"type": "tool", "name": "change_state"}));
    }

    #[test]
    fn tool_choice_auto_passes_through() {
        let req = LLMRequest {
            model: "claude".into(),
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            tool_choice: Some(json!({"type": "auto"})),
            stream: false,
            temperature: None,
        };
        let body = translate_request(&req, false);
        assert_eq!(body.tool_choice.unwrap(), json!({"type": "auto"}));
    }

    #[test]
    fn max_tokens_defaults_when_not_specified() {
        let req = LLMRequest {
            model: "claude".into(),
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            tool_choice: None,
            stream: false,
            temperature: None,
        };
        let body = translate_request(&req, false);
        assert_eq!(body.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn stream_flag_passes_through() {
        let req = make_request(vec![ChatMessage::user("hi")], vec![]);
        assert!(!translate_request(&req, false).stream);
        assert!(translate_request(&req, true).stream);
    }

    // ── SSE parsing ─────────────────────────────────────────────────────

    #[test]
    fn parses_text_delta() {
        let block = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Text("Hello".into())]);
    }

    #[test]
    fn parses_tool_use_block_start() {
        let block = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}";
        let events = parse_sse_block(block);
        assert_eq!(events.len(), 1);
        match &events[0] {
            RawEvent::ToolCall { index, id, name, arguments_fragment } => {
                assert_eq!(*index, 1);
                assert_eq!(id.as_deref(), Some("toolu_1"));
                assert_eq!(name.as_deref(), Some("get_weather"));
                assert!(arguments_fragment.is_none());
            }
            other => panic!("expected ToolCall, got {:?}", other),
        }
    }

    #[test]
    fn parses_tool_use_argument_delta() {
        let block = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"loc\"}}";
        let events = parse_sse_block(block);
        assert_eq!(events.len(), 1);
        match &events[0] {
            RawEvent::ToolCall { index, id, name, arguments_fragment } => {
                assert_eq!(*index, 1);
                assert!(id.is_none());
                assert!(name.is_none());
                assert_eq!(arguments_fragment.as_deref(), Some("{\"loc"));
            }
            other => panic!("expected ToolCall, got {:?}", other),
        }
    }

    #[test]
    fn parses_message_delta_stop_reason() {
        let block = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null}}";
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Finish("tool_use".into())]);
    }

    #[test]
    fn parses_message_stop() {
        let block = "event: message_stop\ndata: {\"type\":\"message_stop\"}";
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Finish("stop".into())]);
    }

    #[test]
    fn ignores_message_start_and_content_block_stop() {
        let start = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}";
        let stop = "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}";
        assert!(parse_sse_block(start).is_empty());
        assert!(parse_sse_block(stop).is_empty());
    }

    #[test]
    fn ignores_blocks_without_event_line() {
        let block = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"x\"}}";
        assert!(parse_sse_block(block).is_empty());
    }

    // ── Response translation ────────────────────────────────────────────

    #[test]
    fn non_streaming_response_with_text_and_tool_use() {
        let body = json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Checking weather"},
                {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"location": "London"}},
            ],
            "stop_reason": "tool_use",
        }).to_string();
        let parsed: AnthropicResponse = serde_json::from_str(&body).unwrap();
        let outcome = translate_response(parsed);
        assert_eq!(outcome.content.as_deref(), Some("Checking weather"));
        let calls = outcome.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].function.name, "get_weather");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["location"], "London");
        assert_eq!(outcome.finish_reason, "tool_use");
    }

    #[test]
    fn non_streaming_text_only_response() {
        let body = json!({
            "content": [{"type": "text", "text": "Just text"}],
            "stop_reason": "end_turn",
        }).to_string();
        let parsed: AnthropicResponse = serde_json::from_str(&body).unwrap();
        let outcome = translate_response(parsed);
        assert_eq!(outcome.content.as_deref(), Some("Just text"));
        assert!(outcome.tool_calls.is_none());
        assert_eq!(outcome.finish_reason, "end_turn");
    }
}