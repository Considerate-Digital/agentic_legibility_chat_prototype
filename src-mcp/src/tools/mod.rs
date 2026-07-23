mod spec_tools;
mod stubs;

use crate::context::AppContext;
use crate::protocol::{McpToolDef, ToolCallResult, ToolContent, UiInteractionRequest};
use serde_json::Value;

pub fn all_tool_defs(ctx: &AppContext) -> Vec<McpToolDef> {
    let mut defs = vec![
        McpToolDef {
            name: "fetch".into(),
            description: "Make a real HTTP request to an external API or URL".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url":     { "type": "string", "description": "Request URL" },
                    "method":  { "type": "string", "description": "HTTP method (default GET)" },
                    "headers": { "type": "string", "description": "JSON object of request headers" },
                    "body":    { "type": "string", "description": "Request body string" }
                },
                "required": ["url"]
            }),
        },
        McpToolDef {
            name: "report_service_step".into(),
            description: "Report which step of a service schema the LLM is currently executing".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "service_id":  { "type": "string", "description": "UUID of the service being executed" },
                    "step_number": { "type": "integer", "description": "1-based step index" },
                    "status":      { "type": "string", "enum": ["starting", "completed", "skipped", "failed"] }
                },
                "required": ["service_id", "step_number", "status"]
            }),
        },
        McpToolDef {
            name: "ui_input".into(),
            description: "Request structured input from the user via the UI".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input_type":  { "type": "string", "enum": ["text", "number", "date", "email", "select"] },
                    "name":        { "type": "string", "description": "Field identifier" },
                    "description": { "type": "string", "description": "Label shown to the user" },
                    "options":     { "type": "array", "items": { "type": "string" }, "description": "Choices for select type" }
                },
                "required": ["input_type", "name", "description"]
            }),
        },
    ];

    if ctx.spec_index.is_some() {
        defs.extend(spec_tools::tool_defs());
    }

    defs
}

pub async fn call_tool(name: &str, args: Value, ctx: &AppContext) -> ToolCallResult {
    // ui_input is special: it populates ui_interaction for Tauri to intercept
    if name == "ui_input" {
        let session_id = format!("ui_{:08X}", stubs::rand_u32());
        let options: Option<Vec<String>> = args["options"].as_array().map(|a| {
            a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        });
        let requested_type = args["input_type"].as_str().unwrap_or("text");
        // A "select" with no usable options can't be rendered as a choice, so it must
        // fall back to a plain text field rather than leave the user with no way to respond.
        let (input_type, options) = match (requested_type, &options) {
            ("select", Some(opts)) if !opts.is_empty() => ("select".to_string(), options),
            ("select", _) => ("text".to_string(), None),
            (other, _) => (other.to_string(), options),
        };
        return ToolCallResult {
            content: vec![ToolContent { content_type: "text".into(), text: "[awaiting user input]".into() }],
            is_error: false,
            ui_interaction: Some(UiInteractionRequest {
                session_id,
                input_type,
                name: args["name"].as_str().unwrap_or("input").to_string(),
                description: args["description"].as_str().unwrap_or("").to_string(),
                options,
            }),
        };
    }

    if let Some(result) = spec_tools::call(name, &args, ctx).await {
        return match result {
            Ok(text) => ToolCallResult {
                content: vec![ToolContent { content_type: "text".into(), text }],
                is_error: false,
                ui_interaction: None,
            },
            Err(text) => ToolCallResult {
                content: vec![ToolContent { content_type: "text".into(), text }],
                is_error: true,
                ui_interaction: None,
            },
        };
    }

    let text = match name {
        "fetch" => stubs::fetch(&args),
        // report_service_step is intercepted by Tauri; this arm is a fallback only
        "report_service_step" => format!(
            "[report_service_step] service_id={} step={} status={}",
            args["service_id"].as_str().unwrap_or("?"),
            args["step_number"],
            args["status"].as_str().unwrap_or("?")
        ),
        other => format!("[STUB] Unknown tool '{}' called with args: {}", other, args),
    };

    ToolCallResult {
        content: vec![ToolContent { content_type: "text".into(), text }],
        is_error: false,
        ui_interaction: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_excludes_spec_tools_when_unconfigured() {
        let ctx = AppContext::disabled_for_test();
        let defs = all_tool_defs(&ctx);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["fetch", "report_service_step", "ui_input"]);
    }

    #[test]
    fn tools_list_includes_spec_tools_when_configured() {
        let tmp = tempfile::TempDir::new().unwrap();
        let index = crate::specs::SpecIndex::empty();
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), index);
        let defs = all_tool_defs(&ctx);
        assert_eq!(defs.len(), 3 + 14);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        for expect in [
            "fetch",
            "report_service_step",
            "ui_input",
            "list_endpoints",
            "get_endpoint",
            "list_services",
            "get_service",
            "list_plans",
            "get_plan",
            "search_specs",
            "specs_for_service",
            "list_service_endpoints",
            "list_plan_endpoints",
            "list_endpoint_services",
            "list_endpoint_plans",
            "get_memory",
            "add_memory",
        ] {
            assert!(names.contains(&expect), "missing tool def: {expect}");
        }
    }

    #[tokio::test]
    async fn call_tool_unknown_name_falls_back_to_stub_message() {
        let ctx = AppContext::disabled_for_test();
        let result = call_tool("totally_unknown", serde_json::json!({}), &ctx).await;
        assert!(!result.is_error);
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn call_tool_gated_tool_errors_when_unconfigured() {
        let ctx = AppContext::disabled_for_test();
        let result = call_tool("list_endpoints", serde_json::json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("unavailable"));
    }
}
