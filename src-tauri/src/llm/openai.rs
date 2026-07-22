use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::config::ProviderConfig;

use super::stream::RawEvent;
use super::types::{
    CompletionOutcome, CompletionResponse, LLMRequest, ToolCallAccum,
};

#[derive(Debug, serde::Serialize, Clone)]
struct ChunkEvent {
    delta: String,
}

/// POST URL for any OpenAI-compatible endpoint.
///
/// `base_url` should be the API root (e.g. `https://api.openai.com/v1` or
/// `https://openrouter.ai/api/v1`), but to be forgiving of users who paste
/// the full endpoint, we also accept and strip a trailing `/chat/completions`.
pub fn endpoint_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let root = trimmed.strip_suffix("/chat/completions").unwrap_or(trimmed);
    format!("{}/chat/completions", root)
}

/// Streaming completion against any OpenAI-compatible endpoint.
pub async fn stream(
    config: &ProviderConfig,
    request: &LLMRequest,
    app: &AppHandle,
) -> Result<CompletionOutcome> {
    let client = build_client()?;
    let url = endpoint_url(&config.base_url);

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(request)
        .send()
        .await
        .context("sending OpenAI-compatible request")?;

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

/// Non-streaming completion against any OpenAI-compatible endpoint.
pub async fn complete(config: &ProviderConfig, request: &LLMRequest) -> Result<CompletionOutcome> {
    let client = build_client()?;
    let url = endpoint_url(&config.base_url);

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(request)
        .send()
        .await
        .context("sending OpenAI-compatible request (non-streaming)")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("LLM API error {} (POST {}): {}", status, url, body));
    }

    let resp: CompletionResponse = response.json().await.context("parsing OpenAI response")?;
    let choice = resp.choices.into_iter().next()
        .ok_or_else(|| anyhow!("LLM returned empty choices"))?;

    Ok(CompletionOutcome {
        content: choice.message.content,
        tool_calls: choice.message.tool_calls,
        finish_reason: choice.finish_reason.unwrap_or_default(),
    })
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .build()
        .context("building HTTP client")
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

/// Parse one SSE block from an OpenAI-compatible streaming response.
pub fn parse_sse_block(block: &str) -> Vec<RawEvent> {
    let mut events = Vec::new();
    for line in block.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data.trim() == "[DONE]" {
            // Treat the explicit DONE sentinel as a finish event so callers
            // can rely on a Finish variant at the end of every stream.
            events.push(RawEvent::Finish("stop".into()));
            continue;
        }
        let Ok(chunk) = serde_json::from_str::<OpenAIStreamChunk>(data) else { continue };
        for choice in chunk.choices {
            if let Some(fr) = choice.finish_reason {
                if !fr.is_empty() {
                    events.push(RawEvent::Finish(fr));
                }
            }
            if let Some(text) = choice.delta.content {
                events.push(RawEvent::Text(text));
            }
            if let Some(partial_calls) = choice.delta.tool_calls {
                for pc in partial_calls {
                    let mut id = None;
                    let mut name = None;
                    let mut arguments_fragment = None;
                    if let Some(func) = pc.function {
                        if let Some(n) = func.name {
                            name = Some(n);
                        }
                        if let Some(a) = func.arguments {
                            arguments_fragment = Some(a);
                        }
                    }
                    if let Some(i) = pc.id {
                        id = Some(i);
                    }
                    if id.is_some() || name.is_some() || arguments_fragment.is_some() {
                        events.push(RawEvent::ToolCall {
                            index: pc.index,
                            id,
                            name,
                            arguments_fragment,
                        });
                    }
                }
            }
        }
    }
    events
}

// ── SSE wire types (OpenAI-compatible) ────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAIDelta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIPartialToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIPartialToolCall {
    index: usize,
    id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: Option<OpenAIPartialFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIPartialFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_from_api_root() {
        assert_eq!(
            endpoint_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions",
        );
        assert_eq!(
            endpoint_url("https://openrouter.ai/api/v1"),
            "https://openrouter.ai/api/v1/chat/completions",
        );
    }

    #[test]
    fn url_trims_trailing_slash() {
        assert_eq!(
            endpoint_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions",
        );
    }

    #[test]
    fn url_strips_existing_chat_completions_suffix() {
        assert_eq!(
            endpoint_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions",
        );
        assert_eq!(
            endpoint_url("https://api.openai.com/v1/chat/completions/"),
            "https://api.openai.com/v1/chat/completions",
        );
    }

    #[test]
    fn url_preserves_prefixed_paths() {
        assert_eq!(
            endpoint_url("https://proxy.example.com/openai/v1"),
            "https://proxy.example.com/openai/v1/chat/completions",
        );
        assert_eq!(
            endpoint_url("https://proxy.example.com/openai/v1/chat/completions"),
            "https://proxy.example.com/openai/v1/chat/completions",
        );
    }

    #[test]
    fn parses_text_delta() {
        let block = r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Text("Hello".into())]);
    }

    #[test]
    fn parses_done_sentinel() {
        let block = "data: [DONE]";
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Finish("stop".into())]);
    }

    #[test]
    fn parses_tool_call_accumulation() {
        let block = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"loc"}}]},"finish_reason":null}]}"#;
        let events = parse_sse_block(block);
        assert_eq!(events.len(), 1);
        match &events[0] {
            RawEvent::ToolCall { index, id, name, arguments_fragment } => {
                assert_eq!(*index, 0);
                assert_eq!(id.as_deref(), Some("call_1"));
                assert_eq!(name.as_deref(), Some("get_weather"));
                assert_eq!(arguments_fragment.as_deref(), Some("{\"loc"));
            }
            other => panic!("expected ToolCall, got {:?}", other),
        }
    }

    #[test]
    fn parses_finish_reason() {
        let block = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Finish("tool_calls".into())]);
    }

    #[test]
    fn ignores_unparseable_lines() {
        let block = "data: not json\ndata: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}";
        let events = parse_sse_block(block);
        assert_eq!(events, vec![RawEvent::Text("ok".into())]);
    }
}