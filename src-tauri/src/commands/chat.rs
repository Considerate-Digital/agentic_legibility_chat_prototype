use std::collections::HashSet;

use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};

use crate::llm::{client, types::ChatMessage};
use crate::llm::types::{LLMFunctionDef, LLMRequest, LLMToolDef};
use crate::mcp::{is_spec_tool, router};
use crate::state_machine::registry::CardSummary;
use crate::ManagedState;

/// Pretty-print the LLM request for `RUST_LOG=legibility_chat=debug`.
///
/// We don't log every message on every call — for long conversations the
/// tool list is what matters most. The system prompt is always included
/// (it's small and tells you *what the model was told to do*).
fn log_llm_request(label: &str, request: &LLMRequest) {
    debug!(
        target: "legibility_chat::llm_request",
        "{label} → model={} stream={} tools={} tool_choice={:?} messages={} temperature={:?}",
        request.model,
        request.stream,
        request
            .tools
            .as_ref()
            .map(|t| t.len())
            .unwrap_or(0),
        request.tool_choice,
        request.messages.len(),
        request.temperature,
    );

    if let Some(tools) = &request.tools {
        for t in tools {
            debug!(target: "legibility_chat::llm_request", "  tool: {}", t.function.name);
        }
    }

    if let Some(sys) = request.messages.iter().find(|m| m.role == "system") {
        if let Some(c) = &sys.content {
            debug!(target: "legibility_chat::llm_request", "  system_prompt:\n{c}");
        }
    }

    // Log the last few user/assistant turns so you can see the model's
    // context window, but not the whole history (which can be huge).
    let tail: Vec<&ChatMessage> = request
        .messages
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    for m in tail {
        let preview = m
            .content
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(200)
            .collect::<String>();
        let tool_calls = m
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|t| t.function.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        debug!(
            target: "legibility_chat::llm_request",
            "  msg[{}] ({}): {} {}",
            m.role,
            preview.len(),
            preview,
            if tool_calls.is_empty() {
                String::new()
            } else {
                format!("[tool_calls={tool_calls}]")
            },
        );
    }
}

fn log_llm_response(label: &str, tool_calls: &[crate::llm::types::ToolCall], text: Option<&str>) {
    match tool_calls.len() {
        0 => {
            let preview: String = text.unwrap_or("").chars().take(200).collect();
            debug!(target: "legibility_chat::llm_response", "{label} → text: {preview}");
        }
        n => {
            info!(
                target: "legibility_chat::llm_response",
                "{label} → {n} tool_call(s): {}",
                tool_calls
                    .iter()
                    .map(|t| format!("{}({})", t.function.name, t.function.arguments))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            for tc in tool_calls {
                info!(
                    target: "legibility_chat::llm_response",
                    "  {} ← id={}",
                    tc.function.name,
                    tc.id,
                );
            }
        }
    }
}

use super::state::state_to_view;

/// Look up which MCP server owns a given bare tool name. Used to label
/// the UI's `tool-called` badge. Falls back to "state" for tools the router
/// doesn't know about (e.g. the in-process `change_state` pseudo-tool).
fn server_for_tool_name(state: &ManagedState, name: &str) -> String {
    if let Ok(router) = state.mcp_router.try_lock() {
        if let Some(router) = router.as_ref() {
            if let Some(s) = router::find_server_for_tool(router, name) {
                return s.to_string();
            }
        }
    }
    if name == "change_state" {
        "state".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Snapshot every server's auto-fetched tool defs (e.g. the "state" server's
/// `tools/list` response, which includes the spec-lookup tools gated on
/// `LIVE_RESOURCES_DIR`) and append them to `state_tools`.
///
/// State-sidecar tools live in `ToolRegistry` and are passed in via
/// `state_tools`. Tools fetched live from the router (currently just the
/// spec-lookup tools) have to be merged in here — otherwise the LLM sees the
/// strengthened prompts ("call `get_service`", "use `list_endpoints`") but
/// those tools are never actually exposed in the `tools` array, so it falls
/// back to whatever the markdown-sourced state tools offer.
///
/// Returns an owned `Vec<LLMToolDef>` with no duplicate `function.name`s.
fn merge_router_tools(
    state: &ManagedState,
    state_tools: Vec<LLMToolDef>,
) -> Vec<LLMToolDef> {
    let mut merged = state_tools;
    let mut seen: HashSet<String> = merged
        .iter()
        .map(|t| t.function.name.clone())
        .collect();

    let router_tools: Vec<LLMToolDef> = {
        // `try_lock` because we're sometimes inside a `read()` lock on
        // `tool_registry` already; if the router isn't ready (still
        // building on startup) we just skip — we'll get the tools on the
        // next loop iteration once setup finishes.
        match state.mcp_router.try_lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|r| r.all_tools())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };

    for tool in router_tools {
        if seen.insert(tool.function.name.clone()) {
            merged.push(tool);
        }
    }
    merged
}

#[tauri::command]
pub async fn send_message(
    content: String,
    state: tauri::State<'_, ManagedState>,
    app: AppHandle,
) -> Result<(), String> {
    {
        let mut conv = state.conversation.write().unwrap();
        conv.push(ChatMessage::user(&content));
    }

    async {
        let cards_enabled = state.config.read().unwrap().cards_enabled;
        let card = if cards_enabled {
            select_card(&state).await?
        } else {
            None
        };
        let eval = evaluate_state(&state, &app).await?;
        run_main_loop(&state, &app).await?;

        // Emit change_state log with card name appended when one was selected
        if let Some(ev) = eval {
            let result = match &card {
                Some(c) => format!("{}\nCard shown: {}", ev.result, c.name),
                None => ev.result,
            };
            app.emit("tool-called", serde_json::json!({
                "name": "change_state",
                "server": "state",
                "args": ev.args,
                "result": result,
            }))
            .ok();
        }

        // Card info travels in llm-done; content is orchestrator-rewritten (falls back to streaming text if absent)
        let done_payload = match &card {
            Some(c) => serde_json::json!({
                "finish_reason": "stop",
                "card": { "name": c.name, "css": c.css, "content": c.content, "context": c.context }
            }),
            None => serde_json::json!({ "finish_reason": "stop" }),
        };
        app.emit("llm-done", done_payload).ok();
        Ok::<_, anyhow::Error>(())
    }
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_conversation(state: tauri::State<'_, ManagedState>) -> Result<(), String> {
    state.conversation.write().unwrap().clear();
    Ok(())
}

// ── Main conversational loop ─────────────────────────────────────────────

async fn run_main_loop(state: &ManagedState, app: &AppHandle) -> anyhow::Result<()> {
    loop {
        let (system_prompt, tool_names, history, provider) = {
            let current = state.current_state.read().unwrap().clone();
            let registry = state.state_registry.read().unwrap();
            let def = registry
                .get(&current)
                .ok_or_else(|| anyhow::anyhow!("no definition for state '{}'", current))?;
            // `change_state` is intentionally included here. The state's
            // system prompt asks the LLM to "use change_state to transition",
            // so the LLM emits function_call(change_state) in its main-loop
            // response. We intercept that call in `dispatch_tool` and route
            // it to `handle_change_state` (the real implementation), not
            // the state sidecar.
            let tool_names: Vec<String> = registry.tools_for_state(&current);
            let history = state.conversation.read().unwrap().clone();
            let provider = state.config.read().unwrap().provider.clone();
            (def.system_prompt.clone(), tool_names, history, provider)
        };

        // Snapshot the registry's tool defs, then merge in any tools the
        // router has auto-fetched (currently just the "state" server's own
        // spec-lookup tools, when `LIVE_RESOURCES_DIR` is set). Without this
        // merge the LLM would only ever see markdown-sourced state tools,
        // even when the sidecar's own `tools/list` lists more in
        // `all_tools()`.
        let tool_defs = {
            let reg = state.tool_registry.read().unwrap();
            reg.to_llm_tools(&tool_names)
        };
        let tool_defs = merge_router_tools(state, tool_defs);

        let mut messages = vec![ChatMessage::system(&system_prompt)];
        messages.extend(history);

        let request = LLMRequest {
            model: provider.model.clone(),
            messages,
            tools: if tool_defs.is_empty() { None } else { Some(tool_defs) },
            tool_choice: None,
            stream: true,
            temperature: Some(0.7),
        };

        log_llm_request("main_loop", &request);
        let outcome = client::stream_completion(&provider, &request, app).await?;
        log_llm_response(
            "main_loop",
            outcome.tool_calls.as_deref().unwrap_or(&[]),
            outcome.content.as_deref(),
        );

        {
            let mut conv = state.conversation.write().unwrap();
            conv.push(ChatMessage {
                role: "assistant".into(),
                content: outcome.content.clone(),
                tool_calls: outcome.tool_calls.clone(),
                tool_call_id: None,
                name: None,
            });
        }

        match outcome.tool_calls {
            None => break,
            Some(tool_calls) => {
                for tc in tool_calls {
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();

                    let result = dispatch_tool(state, app, &tc.function.name, args.clone()).await;

                    // Look up the owning server for the UI badge. Names
                    // surfaced to the LLM are bare (get_service, not
                    // state_get_service), so we resolve server ownership
                    // through the router rather than splitting the name.
                    let server = server_for_tool_name(state, &tc.function.name);

                    info!(
                        target: "legibility_chat::tool_dispatch",
                        "→ {name} ({server}) args={args}",
                        name = tc.function.name,
                    );
                    info!(
                        target: "legibility_chat::tool_dispatch",
                        "← {name} ({server}) result_len={len}",
                        name = tc.function.name,
                        len = result.len(),
                    );

                    app.emit(
                        "tool-called",
                        serde_json::json!({
                            "name": tc.function.name,
                            "server": server,
                            "args": args,
                            "result": result,
                        }),
                    )
                    .ok();

                    state
                        .conversation
                        .write()
                        .unwrap()
                        .push(ChatMessage::tool_result(&tc.id, result));
                }
            }
        }
    }

    Ok(())
}

// ── Mandatory state evaluation — returns result for send_message to emit ──

struct EvalResult {
    args: serde_json::Value,
    result: String,
}

async fn evaluate_state(
    state: &ManagedState,
    app: &AppHandle,
) -> anyhow::Result<Option<EvalResult>> {
    let (current, valid_transitions, history, analyser_provider, change_state_def) = {
        let current = state.current_state.read().unwrap().clone();
        let registry = state.state_registry.read().unwrap();
        let def = registry
            .get(&current)
            .ok_or_else(|| anyhow::anyhow!("no definition for state '{}'", current))?;

        let mut transitions = def.frontmatter.valid_transitions.clone();
        if !transitions.contains(&current) {
            transitions.push(current.clone());
        }

        let history = state.conversation.read().unwrap().clone();
        let analyser_provider = state.config.read().unwrap().analyser_provider();
        let change_state_def = state
            .tool_registry
            .read()
            .unwrap()
            .to_llm_tools(&["change_state".to_string()]);

        (current, transitions, history, analyser_provider, change_state_def)
    };

    let system_prompt = format!(
        "You are a workflow state controller for a UK government services assistant.\n\
        Review the conversation and decide the correct workflow state.\n\n\
        Current state: {name}\n\
        Valid options (including staying): {options}\n\n\
        You MUST call `change_state`. Call it with the current state if it remains appropriate.",
        name = current,
        options = valid_transitions.join(", "),
    );

    let mut messages = vec![ChatMessage::system(&system_prompt)];
    messages.extend(history);

    let request = LLMRequest {
        model: analyser_provider.model.clone(),
        messages,
        tools: Some(change_state_def),
        tool_choice: Some(serde_json::json!({
            "type": "function",
            "function": { "name": "change_state" }
        })),
        stream: false,
        temperature: Some(0.0),
    };

    log_llm_request("analyser", &request);
    let outcome = client::complete(&analyser_provider, &request).await?;
    log_llm_response(
        "analyser",
        outcome.tool_calls.as_deref().unwrap_or(&[]),
        outcome.content.as_deref(),
    );

    if let Some(tool_calls) = outcome.tool_calls {
        for tc in tool_calls {
            // The LLM sees bare tool names ("change_state"). The handler
            // expects the {target_state, reason} argument schema.
            if tc.function.name == "change_state" {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                info!(
                    target: "legibility_chat::tool_dispatch",
                    "→ analyser.change_state args={}",
                    args,
                );
                let result = handle_change_state(state, app, &args);
                return Ok(Some(EvalResult { args, result }));
            }
        }
    }

    Ok(None)
}

// ── Helpers ───────────────────────────────────────────────────────────────

pub fn handle_change_state(
    state: &ManagedState,
    app: &AppHandle,
    args: &serde_json::Value,
) -> String {
    let target_str = args["target_state"].as_str().unwrap_or("").to_string();
    let reason = args["reason"].as_str().unwrap_or("").to_string();

    let current = state.current_state.read().unwrap().clone();

    let (is_known, can_transition, valid_str) = {
        let registry = state.state_registry.read().unwrap();
        let is_known = registry.get(&target_str).is_some();
        let can = registry.can_transition(&current, &target_str);
        let valid = registry
            .get(&current)
            .map(|d| d.frontmatter.valid_transitions.join(", "))
            .unwrap_or_default();
        (is_known, can, valid)
    };

    if !is_known {
        return format!("Error: '{}' is not a recognised state.", target_str);
    }

    if target_str != current && !can_transition {
        return format!(
            "Error: cannot transition from {} to {}. Valid transitions: {}",
            current, target_str, valid_str
        );
    }

    if target_str != current {
        *state.current_state.write().unwrap() = target_str.clone();
    }

    if let Some(view) = state_to_view(state, &target_str) {
        app.emit("state-changed", &view).ok();
    }

    if target_str == current {
        format!("State confirmed as {}. Reason: {}", target_str, reason)
    } else {
        format!("State changed to {}. Reason: {}", target_str, reason)
    }
}

// ── Card selection — picks card and rewrites content to fit its format ────

struct SelectedCard {
    name: String,
    css: Option<String>,
    content: Option<String>,
    context: Option<String>,
}

async fn select_card(state: &ManagedState) -> anyhow::Result<Option<SelectedCard>> {
    let (current, card_summaries, history, analyser_provider) = {
        let current = state.current_state.read().unwrap().clone();
        let registry = state.card_registry.read().unwrap();
        if registry.is_empty() {
            return Ok(None);
        }
        let summaries = registry.all_summaries();
        let history = state.conversation.read().unwrap().clone();
        let provider = state.config.read().unwrap().analyser_provider();
        (current, summaries, history, provider)
    };

    let cards_list = card_summaries
        .iter()
        .map(|c| {
            format!(
                "[{}] — {}\n  When selected, format content as: {}",
                c.name, c.description, c.generation_instructions
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt = format!(
        "Review the conversation including the assistant's latest response.\n\
        Decide whether to present it using a contextual UI card.\n\
        If selecting a card, rewrite the assistant's response to better suit the card's format.\n\n\
        Current state: {state}\n\n\
        Available cards:\n{cards}\n\n\
        Call `select_card` with the most appropriate card, or card_name=\"none\" if no card fits.\n\
        When selecting a card, populate `content` with the reformatted response.",
        state = current,
        cards = cards_list,
    );

    let mut messages = vec![ChatMessage::system(&system_prompt)];
    messages.extend(history);

    let tool = build_select_card_tool(&card_summaries);

    let request = LLMRequest {
        model: analyser_provider.model.clone(),
        messages,
        tools: Some(vec![tool]),
        tool_choice: Some(serde_json::json!({
            "type": "function",
            "function": { "name": "select_card" }
        })),
        stream: false,
        temperature: Some(0.0),
    };

    log_llm_request("card_selector", &request);
    let outcome = client::complete(&analyser_provider, &request).await?;
    log_llm_response(
        "card_selector",
        outcome.tool_calls.as_deref().unwrap_or(&[]),
        outcome.content.as_deref(),
    );

    if let Some(tool_calls) = outcome.tool_calls {
        for tc in tool_calls {
            if tc.function.name == "select_card" {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                let card_name = args["card_name"].as_str().unwrap_or("none");
                info!(
                    target: "legibility_chat::tool_dispatch",
                    "→ card_selector.select_card name={card_name}",
                );

                if card_name != "none" {
                    let css = state
                        .card_registry
                        .read()
                        .unwrap()
                        .get(card_name)
                        .and_then(|d| d.css.clone());

                    let content = args["content"].as_str().map(|s| s.to_string());
                    let context = args["context"].as_str().map(|s| s.to_string());

                    return Ok(Some(SelectedCard {
                        name: card_name.to_string(),
                        css,
                        content,
                        context,
                    }));
                }
            }
        }
    }

    Ok(None)
}

fn build_select_card_tool(summaries: &[CardSummary]) -> LLMToolDef {
    let names: Vec<serde_json::Value> = summaries
        .iter()
        .map(|c| serde_json::Value::String(c.name.clone()))
        .chain(std::iter::once(serde_json::Value::String("none".to_string())))
        .collect();

    LLMToolDef {
        def_type: "function".to_string(),
        function: LLMFunctionDef {
            name: "select_card".to_string(),
            description: "Select which UI card should wrap the assistant response, or \"none\". If selecting a card, rewrite the response to suit the card format.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "card_name": {
                        "type": "string",
                        "description": "Name of the card to use, or \"none\"",
                        "enum": names
                    },
                    "context": {
                        "type": "string",
                        "description": "1–2 sentence summary of what the user asked or discussed, providing conversational context shown above the card body. Always include this when selecting a card."
                    },
                    "content": {
                        "type": "string",
                        "description": "Rewritten assistant response formatted for this card. Required when card_name is not \"none\"."
                    },
                    "reason": {
                        "type": "string",
                        "description": "Brief reason for the choice"
                    }
                },
                "required": ["card_name", "context", "reason"]
            }),
        },
    }
}


#[derive(serde::Serialize, Clone)]
struct ServiceStepEvent {
    service_id: String,
    service_name: String,
    step_number: u64,
    total_steps: usize,
    endpoint_id: String,
    endpoint_name: String,
    department: String,
    required: bool,
    status: String,
}

fn handle_report_service_step(
    state: &ManagedState,
    app: &AppHandle,
    args: &serde_json::Value,
) -> String {
    let service_id = match args["service_id"].as_str() {
        Some(id) => id.to_string(),
        None => return "[report_service_step] missing service_id".to_string(),
    };
    let step_number = match args["step_number"].as_u64() {
        Some(n) if n >= 1 => n,
        _ => return "[report_service_step] step_number must be a positive integer".to_string(),
    };
    let status = args["status"].as_str().unwrap_or("starting").to_string();

    let live_resources_dir = match state.config.read().unwrap().live_resources_dir.clone() {
        Some(d) => d,
        None => return "[report_service_step] live_resources_dir is not configured".to_string(),
    };

    let services_dir = std::path::Path::new(&live_resources_dir).join("services");
    let entries = match std::fs::read_dir(&services_dir) {
        Ok(e) => e,
        Err(e) => return format!("[report_service_step] cannot read services dir: {}", e),
    };

    // Scan service files to find the one with the matching id
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Inline split_frontmatter logic
        let content_trimmed = content.trim_start();
        let rest = match content_trimmed
            .strip_prefix("---\n")
            .or_else(|| content_trimmed.strip_prefix("---\r\n"))
        {
            Some(r) => r,
            None => continue,
        };
        let end_pos = match rest.find("\n---") {
            Some(p) => p,
            None => continue,
        };
        let yaml = &rest[..end_pos];
        let body = rest[end_pos + 4..].trim_start_matches('\n').trim_start_matches('\r');

        // Extract id and name from YAML frontmatter (simple line-by-line)
        let mut file_id = String::new();
        let mut service_name = String::new();
        for line in yaml.lines() {
            if let Some(val) = line.strip_prefix("id:") {
                file_id = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("name:") {
                service_name = val.trim().to_string();
            }
        }

        if file_id != service_id {
            continue;
        }

        // Parse numbered step lines: "N. uuid, name, department, required|optional"
        let steps: Vec<(u64, String, String, String, bool)> = body
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let dot_pos = line.find('.')?;
                let num: u64 = line[..dot_pos].trim().parse().ok()?;
                let rest = line[dot_pos + 1..].trim();
                let parts: Vec<&str> = rest.splitn(4, ", ").collect();
                if parts.len() < 3 {
                    return None;
                }
                let endpoint_id = parts[0].trim().to_string();
                let endpoint_name = parts[1].trim().to_string();
                let department = parts[2].trim().to_string();
                let required = parts.get(3).map(|s| s.trim() == "required").unwrap_or(false);
                Some((num, endpoint_id, endpoint_name, department, required))
            })
            .collect();

        let total_steps = steps.len();
        let step = steps.iter().find(|(n, ..)| *n == step_number);

        let (endpoint_id, endpoint_name, department, required) = match step {
            Some((_, eid, ename, dept, req)) => (eid.clone(), ename.clone(), dept.clone(), *req),
            None => return format!(
                "[report_service_step] step {} not found in service '{}' ({} steps total)",
                step_number, service_name, total_steps
            ),
        };

        let event = ServiceStepEvent {
            service_id: service_id.clone(),
            service_name: service_name.clone(),
            step_number,
            total_steps,
            endpoint_id,
            endpoint_name: endpoint_name.clone(),
            department: department.clone(),
            required,
            status: status.clone(),
        };
        app.emit("service-step-changed", &event).ok();

        return format!(
            "Step {}/{}: {} ({}) — {} [{}]",
            step_number, total_steps, endpoint_name, department,
            if required { "required" } else { "optional" },
            status
        );
    }

    format!("[report_service_step] service '{}' not found in {}", service_id, services_dir.display())
}

#[derive(serde::Serialize, Clone)]
struct UiInputRequestedEvent {
    input_type: String,
    name: String,
    description: String,
    options: Option<Vec<String>>,
}

async fn handle_ui_input(
    state: &ManagedState,
    app: &AppHandle,
    args: &serde_json::Value,
) -> String {
    let input_type = args["input_type"].as_str().unwrap_or("text").to_string();
    let name = args["name"].as_str().unwrap_or("input").to_string();
    let description = args["description"].as_str().unwrap_or("").to_string();
    let options = args["options"].as_array().map(|a| {
        a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
    });

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    *state.pending_ui_input.lock().await = Some(tx);

    app.emit("ui-input-requested", UiInputRequestedEvent {
        input_type,
        name,
        description,
        options,
    }).ok();

    // LLM loop pauses here until the user submits via submit_ui_input
    rx.await.unwrap_or_else(|_| "[ui_input cancelled]".to_string())
}

#[tauri::command]
pub async fn submit_ui_input(
    value: String,
    state: tauri::State<'_, ManagedState>,
) -> Result<(), String> {
    if let Some(tx) = state.pending_ui_input.lock().await.take() {
        let _ = tx.send(value);
    }
    Ok(())
}

async fn dispatch_tool(
    state: &ManagedState,
    app: &AppHandle,
    name: &str,
    args: serde_json::Value,
) -> String {
    // `change_state` is a pseudo-tool exposed to the LLM so it can
    // signal state transitions. The real implementation is in
    // `handle_change_state` (which mutates `state.current_state`); the
    // legibility-chat-mcp sidecar does NOT have a real `change_state`, so we
    // intercept the LLM's call here before it reaches the router.
    if name == "change_state" {
        return handle_change_state(state, app, &args);
    }

    // `report_service_step` reads the live_resources service schema and
    // emits a progress event to the UI. Intercepted here because the sidecar
    // has no access to live_resources files.
    if name == "report_service_step" {
        return handle_report_service_step(state, app, &args);
    }

    // `ui_input` pauses the LLM loop and waits for user input via the UI.
    if name == "ui_input" {
        return handle_ui_input(state, app, &args).await;
    }

    // Friendly fallback: when the LLM calls a spec-lookup tool but the
    // router has no server owning it, the most likely cause is that
    // `live_resources_dir` is unset (so the sidecar never registered these
    // gated tools). Replace the cryptic router error with an actionable
    // message.
    if is_spec_tool(name) {
        let live_resources_dir_set = state
            .config
            .read()
            .unwrap()
            .live_resources_dir
            .is_some();
        if !live_resources_dir_set {
            warn!(
                target: "legibility_chat::tool_dispatch",
                "✗ spec tool '{name}' called but live_resources_dir is unset",
            );
            return format!(
                "The `{name}` tool needs a Live Resources directory to be configured. \
                 Open **Setup** from the top bar to pick one."
            );
        }
    }

    let router = state.mcp_router.lock().await;
    match router.as_ref() {
        Some(r) => match r.dispatch(name, args).await {
            Ok(result) => result,
            Err(e) => {
                // Distinct log line so you can tell "model didn't call the
                // right tool" from "model called a tool we don't have".
                warn!(
                    target: "legibility_chat::tool_dispatch",
                    "✗ router rejected '{name}': {e}",
                );
                format!("Tool error: {}", e)
            }
        },
        None => {
            warn!(
                target: "legibility_chat::tool_dispatch",
                "✗ router not initialised when dispatching '{name}'",
            );
            format!("Error: MCP router not initialised (tool '{}')", name)
        }
    }
}
