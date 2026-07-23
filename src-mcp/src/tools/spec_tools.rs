//! Read-only spec-lookup tools, ported from legibility-mcp's rmcp-macro
//! tools (`../mcp/src/tools.rs`) into plain functions matching this crate's
//! hand-written MCP style, plus two new memory tools.
//!
//! All 14 tools here are gated on `AppContext::spec_index` being `Some` —
//! see `call()`, which returns `None` for unrecognised names (letting the
//! caller fall through to the always-on stub tools) and `Some(Err(..))`
//! when the name is recognised but the specs directory isn't configured.

use serde_json::{json, Value};

use crate::context::AppContext;
use crate::protocol::McpToolDef;
use crate::specs::{search as spec_search, Doc, DocKind, SpecIndex};
use convert_case::ccase;

const TOOL_NAMES: &[&str] = &[
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
];

/// Dispatch a tool call by name. Returns `None` if `name` isn't one of the
/// 14 spec/memory tools (the caller should try other tool tables). Returns
/// `Some(Err(..))` if the name is recognised but `ctx.spec_index` is `None`.
pub async fn call(name: &str, args: &Value, ctx: &AppContext) -> Option<Result<String, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    if ctx.spec_index.is_none() {
        return Some(Err(format!(
            "tool '{name}' is unavailable: LIVE_RESOURCES_DIR is not configured, or the initial specs scan failed"
        )));
    }

    let result = match name {
        "list_endpoints" => list_endpoints(ctx, args).await,
        "get_endpoint" => get_endpoint(ctx, args).await,
        "list_services" => list_services(ctx).await,
        "get_service" => get_service(ctx, args).await,
        "list_plans" => list_plans(ctx).await,
        "get_plan" => get_plan(ctx, args).await,
        "search_specs" => search_specs(ctx, args).await,
        "specs_for_service" => specs_for_service(ctx, args).await,
        "list_service_endpoints" => list_service_endpoints(ctx, args).await,
        "list_plan_endpoints" => list_plan_endpoints(ctx, args).await,
        "list_endpoint_services" => list_endpoint_services(ctx, args).await,
        "list_endpoint_plans" => list_endpoint_plans(ctx, args).await,
        "get_memory" => get_memory(ctx),
        "add_memory" => add_memory(ctx, args),
        _ => unreachable!("checked against TOOL_NAMES above"),
    };
    Some(result)
}

// ---------------------------------------------------------------------------
// Tool definitions (descriptions/schemas reworded from legibility-mcp where
// they referenced the now-deleted `lookup_service` tool)
// ---------------------------------------------------------------------------

pub fn tool_defs() -> Vec<McpToolDef> {
    vec![
        McpToolDef {
            name: "list_endpoints".into(),
            description: "List all API endpoints available in the spec corpus, with each one's name, HTTP method, URL, and the service/plan that references it. Optionally filtered by a case-insensitive substring.\n\nCall this when the user asks 'what endpoints are available?', wants to browse, or wants to find endpoints matching a topic ('endpoints related to authentication', 'endpoints in the payment service', 'what does the Resource Auth Service expose?'). Returns a JSON array of endpoint summaries.\n\nDo NOT call this for full details on a single endpoint — use get_endpoint with the endpoint's name for that. Do NOT call this for benefits/policy lookup (PIP, Universal Credit, etc.) — use search_specs/get_service for the spec corpus, or general knowledge for benefits/policy content outside it.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Optional case-insensitive substring filter matched against the endpoint's file stem, front-matter name, HTTP method, URL path, department, and ID. Omit to return every endpoint." }
                },
                "required": []
            }),
        },
        McpToolDef {
            name: "get_endpoint".into(),
            description: "Fetch the full content of one specific API endpoint, including HTTP method, URL, request/response examples, parameters, headers, error codes, and any narrative description.\n\nCall this when the user asks about a specific endpoint — 'show me the endpoints for this service', 'what does get_driving_licence return?', 'what params does post_share_code need?', 'how do I call submit_form?'. Pass the endpoint's file stem name in snake case (e.g. 'post_share_code' rather than 'post-share-code', 'get_driving_licence' instead of 'get-driving-licence').\n\nDo NOT call this when you don't yet know which endpoint you want — use list_endpoints or search_specs first to find the right one. Do NOT call this for benefits/policy lookup.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Endpoint file stem, e.g. `getDrivingLicence`." }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_services".into(),
            description: "List all available technical services in the spec corpus, with each one's name, owner department, lifecycle status, and a count of endpoints it references.\n\nCall this when the user asks 'what services are available?', 'what services exist?', or wants to browse the service catalog. Returns a JSON array of service summaries.\n\nDo NOT call this for full service details — use get_service with a specific service name. Do NOT call this for benefits/policy lookup (PIP, Universal Credit, etc. — those are NOT services in this corpus; use search_specs/get_service for the spec corpus, or general knowledge for benefits/policy content outside it).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        McpToolDef {
            name: "get_service".into(),
            description: "Fetch the full content of one specific technical service — its description, owner department, lifecycle status, list of endpoints it references, and any narrative content.\n\nCall this when the user asks about a specific service by name — 'tell me about the Resource Auth Service', 'what does the Driving Licence service do?', 'who owns the Benefits service?', 'show me what the Auth service exposes'.\n\nDo NOT call this for benefits/policy lookup (PIP, Universal Credit, Carer's Allowance — those are NOT services in this corpus; use search_specs/get_service for the spec corpus, or general knowledge for benefits/policy content outside it). Do NOT call this when you don't know the exact name — use list_services first.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_plans".into(),
            description: "List all available plans (multi-step government interactions) in the spec corpus, with each one's name, lifecycle status, owner department, type, and a count of endpoints it references.\n\nCall this when the user asks 'what plans are available?', 'what multi-step processes exist?', or wants to browse plans. Returns a JSON array of plan summaries.\n\nDo NOT call this for full plan details — use get_plan with a specific plan name. Do NOT call this for benefits/policy lookup.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        McpToolDef {
            name: "get_plan".into(),
            description: "Fetch the full content of one specific plan — the steps, dependencies, and referenced endpoints for a multi-step government interaction (e.g. 'apply for PIP', 'register a death', 'claim Child Benefit').\n\nCall this when the user asks about a specific named plan or wants the steps for one — 'show me the apply-for-PIP plan', 'what are the steps in plan X?', 'what endpoints does this plan use?'.\n\nDo NOT call this for single services or endpoints — use get_service or get_endpoint for those. Do NOT call this for benefits/policy lookup.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "search_specs".into(),
            description: "Case-insensitive substring search across all spec documents (endpoints, services, plans). Returns matching lines with file paths and line numbers. Optionally filter by kind: 'endpoint', 'service', or 'plan'.\n\nCall this when the user has an open-ended question about the spec corpus — 'anything related to authentication?', 'where is X mentioned?', 'find anything about the Resource Auth Service', 'find endpoints that mention MFA'.\n\nThis is the broadest tool. Prefer list_endpoints, get_endpoint, list_services, get_service, get_plan, or list_plans when you already know what kind of thing you're looking for. Use search_specs only when the question is 'find me anything that mentions X' or when you don't know which kind of spec to query.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Substring (or regex, if `rg` is used) to search for." },
                    "kind": { "type": "string", "description": "Optional kind filter: `endpoint`, `service`, or `plan`." }
                },
                "required": ["query"]
            }),
        },
        McpToolDef {
            name: "specs_for_service".into(),
            description: "Return the service document plus all of the endpoints it references (resolved through the cross-reference index), concatenated with --- separators.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_service_endpoints".into(),
            description: "List the endpoints that a service references in its body (resolved through the cross-reference index).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_plan_endpoints".into(),
            description: "List the endpoints that a plan references in its body (resolved through the cross-reference index).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_endpoint_services".into(),
            description: "List the services whose bodies reference an endpoint.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "list_endpoint_plans".into(),
            description: "List the plans whose bodies reference an endpoint.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
        },
        McpToolDef {
            name: "get_memory".into(),
            description: "Read previously recorded facts about this user/case from persistent memory. Call at the start of a conversation to recall prior context.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        McpToolDef {
            name: "add_memory".into(),
            description: "Record a durable fact learned from explicit user input or a successful interaction (e.g. a completed step, a stated preference, a confirmed case detail), so future sessions can recall it. Do not call for speculative or unconfirmed information.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fact": { "type": "string", "description": "The fact to record, in a single self-contained sentence." }
                },
                "required": ["fact"]
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_endpoints(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let filter = args["filter"].as_str();
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    let endpoints: Vec<_> = index
        .endpoints(filter)
        .into_iter()
        .map(|d| endpoint_summary(&index, d))
        .collect();
    serde_json::to_string_pretty(&endpoints).map_err(|e| e.to_string())
}

async fn get_endpoint(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("get_endpoint requires a 'name' string argument")?;
    let name = ccase!(snake, &name);
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    index
        .get(DocKind::Endpoint, None, &name)
        .map(|d| d.raw.clone())
        .ok_or_else(|| not_found("endpoint", &name))
}

async fn list_services(ctx: &AppContext) -> Result<String, String> {
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    let services: Vec<_> = index
        .services()
        .into_iter()
        .map(|doc| {
            json!({
                "name": doc.name,
                "display_name": doc.display_name(),
                "endpoint_count": index.resolved_endpoint_count(&doc.name),
                "status": doc.frontmatter.status,
                "owner": doc.frontmatter.owner,
            })
        })
        .collect();
    serde_json::to_string_pretty(&services).map_err(|e| e.to_string())
}

async fn get_service(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("get_service requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    index
        .get(DocKind::Service, None, name)
        .map(|d| d.body.clone())
        .ok_or_else(|| not_found("service", name))
}

async fn list_plans(ctx: &AppContext) -> Result<String, String> {
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    let plans: Vec<_> = index
        .plans()
        .into_iter()
        .map(|doc| {
            json!({
                "name": doc.name,
                "display_name": doc.display_name(),
                "endpoint_count": index.resolved_endpoint_count(&doc.name),
                "status": doc.frontmatter.status,
                "owner": doc.frontmatter.owner,
                "type": doc.frontmatter.r#type,
            })
        })
        .collect();
    serde_json::to_string_pretty(&plans).map_err(|e| e.to_string())
}

async fn get_plan(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("get_plan requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    index
        .get(DocKind::Plan, None, name)
        .map(|d| d.body.clone())
        .ok_or_else(|| not_found("plan", name))
}

async fn search_specs(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let query = args["query"]
        .as_str()
        .ok_or("search_specs requires a 'query' string argument")?;
    let kind = parse_kind(args["kind"].as_str())?;
    let live_dir = ctx.live_resources_dir.as_ref().unwrap();
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    let hits = spec_search::search(live_dir, &index, query, kind)
        .await
        .map_err(|e| format!("search failed: {e:#}"))?;
    let payload: Vec<_> = hits
        .into_iter()
        .map(|h| {
            json!({
                "file": h.file.to_string_lossy(),
                "line": h.line,
                "text": h.text,
            })
        })
        .collect();
    serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())
}

async fn specs_for_service(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("specs_for_service requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    let docs = index.for_service(name);
    if docs.is_empty() {
        return Err(not_found("service", name));
    }
    let mut buf = String::new();
    for (i, doc) in docs.iter().enumerate() {
        if i > 0 {
            buf.push_str("\n\n---\n\n");
        }
        buf.push_str(&format!(
            "# {} ({})\n\n",
            doc.display_name(),
            kind_label(doc.kind)
        ));
        buf.push_str(&doc.body);
    }
    Ok(buf)
}

async fn list_service_endpoints(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("list_service_endpoints requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    if index.get(DocKind::Service, None, name).is_none() {
        return Err(not_found("service", name));
    }
    let endpoints: Vec<_> = index
        .endpoints_for_service(name)
        .into_iter()
        .map(|d| endpoint_summary(&index, d))
        .collect();
    serde_json::to_string_pretty(&endpoints).map_err(|e| e.to_string())
}

async fn list_plan_endpoints(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("list_plan_endpoints requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    if index.get(DocKind::Plan, None, name).is_none() {
        return Err(not_found("plan", name));
    }
    let endpoints: Vec<_> = index
        .endpoints_for_plan(name)
        .into_iter()
        .map(|d| endpoint_summary(&index, d))
        .collect();
    serde_json::to_string_pretty(&endpoints).map_err(|e| e.to_string())
}

async fn list_endpoint_services(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("list_endpoint_services requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    if index.get(DocKind::Endpoint, None, name).is_none() {
        return Err(not_found("endpoint", name));
    }
    let services: Vec<_> = index
        .services_for_endpoint(name)
        .into_iter()
        .map(|doc| {
            json!({
                "name": doc.name,
                "display_name": doc.display_name(),
                "status": doc.frontmatter.status,
            })
        })
        .collect();
    serde_json::to_string_pretty(&services).map_err(|e| e.to_string())
}

async fn list_endpoint_plans(ctx: &AppContext, args: &Value) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("list_endpoint_plans requires a 'name' string argument")?;
    let index = ctx.spec_index.as_ref().unwrap().read().await;
    if index.get(DocKind::Endpoint, None, name).is_none() {
        return Err(not_found("endpoint", name));
    }
    let plans: Vec<_> = index
        .plans_for_endpoint(name)
        .into_iter()
        .map(|doc| {
            json!({
                "name": doc.name,
                "display_name": doc.display_name(),
                "status": doc.frontmatter.status,
                "type": doc.frontmatter.r#type,
            })
        })
        .collect();
    serde_json::to_string_pretty(&plans).map_err(|e| e.to_string())
}

/// Read `<live_resources_dir>/memory.md` verbatim. This file is a flat file
/// at the ROOT of the live-resources tree (a sibling of `endpoints/`,
/// `services/`, `plans/`), so the spec loader's directory scan never touches
/// it.
fn get_memory(ctx: &AppContext) -> Result<String, String> {
    let dir = ctx.live_resources_dir.as_ref().unwrap();
    let path = dir.join("memory.md");
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok("(no memory recorded yet)".to_string())
        }
        Err(e) => Err(format!("failed to read memory.md: {e}")),
    }
}

/// Append `- {fact}\n` to `<live_resources_dir>/memory.md`, creating the
/// file (but not the parent directory) on first use. Append-only — never
/// overwrites.
fn add_memory(ctx: &AppContext, args: &Value) -> Result<String, String> {
    use std::io::Write;

    let fact = args["fact"]
        .as_str()
        .ok_or("add_memory requires a 'fact' string argument")?;
    let dir = ctx.live_resources_dir.as_ref().unwrap();
    let path = dir.join("memory.md");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("failed to open memory.md: {e}"))?;
    file.write_all(format!("- {fact}\n").as_bytes())
        .map_err(|e| format!("failed to write memory.md: {e}"))?;
    Ok(format!("recorded: {fact}"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn endpoint_summary(index: &SpecIndex, doc: &Doc) -> Value {
    json!({
        "name": doc.name,
        "display_name": doc.display_name(),
        "method": doc.frontmatter.method,
        "path": doc.frontmatter.endpoint,
        "tags": doc.frontmatter.tags,
        "id": doc.frontmatter.id,
        "department": doc.frontmatter.department,
        "referenced_by": {
            "services": index
                .services_for_endpoint(&doc.name)
                .into_iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
            "plans": index
                .plans_for_endpoint(&doc.name)
                .into_iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>(),
        },
    })
}

fn not_found(kind: &str, name: &str) -> String {
    format!("{kind} not found: {name}")
}

fn kind_label(k: DocKind) -> &'static str {
    match k {
        DocKind::Endpoint => "endpoint",
        DocKind::Service => "service",
        DocKind::Plan => "plan",
    }
}

fn parse_kind(kind: Option<&str>) -> Result<Option<DocKind>, String> {
    match kind {
        None => Ok(None),
        Some("endpoint") => Ok(Some(DocKind::Endpoint)),
        Some("service") => Ok(Some(DocKind::Service)),
        Some("plan") => Ok(Some(DocKind::Plan)),
        Some(other) => Err(format!(
            "unknown kind '{other}'; expected endpoint, service, or plan"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &std::path::Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    /// Builds a tempdir with one endpoint, one service (referencing it), and
    /// one plan (referencing it), and an `AppContext` wrapping the scan.
    fn sample_ctx() -> (TempDir, AppContext) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nid: ep-1\nname: Get Foo\nmethod: GET\nendpoint: https://api.example.com/foo\n---\nendpoint body",
        );
        write(
            &root.join("services"),
            "driver.md",
            "---\nname: Driver\nstatus: published\nowner: alice\n---\n1. ep-1, getFoo, DVLA\n",
        );
        write(
            &root.join("plans"),
            "apply.md",
            "---\nname: Apply\nstatus: draft\ntype: plan\n---\n1. ep-1, getFoo, DVLA\n",
        );
        let index = SpecIndex::scan(root).unwrap();
        let ctx = AppContext::for_test(root.to_path_buf(), index);
        (tmp, ctx)
    }

    #[tokio::test]
    async fn list_endpoints_returns_summaries() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("list_endpoints", &json!({}), &ctx).await.unwrap().unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "getFoo");
        assert_eq!(arr[0]["referenced_by"]["services"][0], "driver");
        assert_eq!(arr[0]["referenced_by"]["plans"][0], "apply");
    }

    #[tokio::test]
    async fn get_endpoint_returns_body_or_not_found() {
        let (_tmp, ctx) = sample_ctx();
        let ok = call("get_endpoint", &json!({"name": "getFoo"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ok, "endpoint body");

        let err = call("get_endpoint", &json!({"name": "missing"}), &ctx)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(err, "endpoint not found: missing");
    }

    #[tokio::test]
    async fn list_services_includes_endpoint_count() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("list_services", &json!({}), &ctx).await.unwrap().unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["name"], "driver");
        assert_eq!(arr[0]["endpoint_count"], 1);
        assert_eq!(arr[0]["owner"], "alice");
    }

    #[tokio::test]
    async fn get_service_returns_body() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("get_service", &json!({"name": "driver"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        assert!(body.contains("ep-1, getFoo, DVLA"));
    }

    #[tokio::test]
    async fn specs_for_service_concatenates_service_and_endpoints() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("specs_for_service", &json!({"name": "driver"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        assert!(body.contains("# Driver (service)"));
        assert!(body.contains("# Get Foo (endpoint)"));
        assert!(body.contains("---"));
    }

    #[tokio::test]
    async fn list_service_endpoints_resolves_refs() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("list_service_endpoints", &json!({"name": "driver"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.as_array().unwrap()[0]["name"], "getFoo");

        let err = call("list_service_endpoints", &json!({"name": "nope"}), &ctx)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(err, "service not found: nope");
    }

    #[tokio::test]
    async fn list_plan_endpoints_resolves_refs() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("list_plan_endpoints", &json!({"name": "apply"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.as_array().unwrap()[0]["name"], "getFoo");
    }

    #[tokio::test]
    async fn list_endpoint_services_and_plans() {
        let (_tmp, ctx) = sample_ctx();
        let services = call("list_endpoint_services", &json!({"name": "getFoo"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&services).unwrap();
        assert_eq!(parsed.as_array().unwrap()[0]["name"], "driver");

        let plans = call("list_endpoint_plans", &json!({"name": "getFoo"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&plans).unwrap();
        assert_eq!(parsed.as_array().unwrap()[0]["name"], "apply");

        let err = call("list_endpoint_services", &json!({"name": "nope"}), &ctx)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(err, "endpoint not found: nope");
    }

    #[tokio::test]
    async fn search_specs_fallback_finds_matches() {
        let (_tmp, ctx) = sample_ctx();
        let body = call("search_specs", &json!({"query": "endpoint body"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["text"].as_str().unwrap().contains("endpoint body"));
    }

    #[tokio::test]
    async fn search_specs_rejects_unknown_kind() {
        let (_tmp, ctx) = sample_ctx();
        let err = call("search_specs", &json!({"query": "x", "kind": "bogus"}), &ctx)
            .await
            .unwrap()
            .unwrap_err();
        assert!(err.contains("unknown kind"));
    }

    /// Exercises the `rg`-backed path when `rg` is actually on `PATH`. On
    /// this dev machine `rg` is not installed, so this test no-ops rather
    /// than failing — the fallback path above (and `search.rs`'s own
    /// ported `substring_fallback_finds_matches` test) cover the logic when
    /// it isn't.
    #[tokio::test]
    async fn search_specs_rg_path_when_available() {
        if std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping search_specs_rg_path_when_available: rg not on PATH");
            return;
        }
        let (_tmp, ctx) = sample_ctx();
        let body = call("search_specs", &json!({"query": "endpoint body"}), &ctx)
            .await
            .unwrap()
            .unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn all_gated_tools_error_when_spec_index_absent() {
        let ctx = AppContext::disabled_for_test();
        for name in TOOL_NAMES {
            let result = call(name, &json!({}), &ctx).await;
            assert!(result.is_some(), "{name} should be recognised");
            assert!(result.unwrap().is_err(), "{name} should error when unconfigured");
        }
    }

    #[tokio::test]
    async fn unknown_tool_name_returns_none() {
        let (_tmp, ctx) = sample_ctx();
        assert!(call("not_a_real_tool", &json!({}), &ctx).await.is_none());
    }

    #[test]
    fn get_memory_missing_file_returns_placeholder() {
        let tmp = TempDir::new().unwrap();
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), SpecIndex::empty());
        assert_eq!(get_memory(&ctx).unwrap(), "(no memory recorded yet)");
    }

    #[test]
    fn get_memory_existing_file_returns_verbatim() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "memory.md", "- likes short answers\n- has case #123\n");
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), SpecIndex::empty());
        assert_eq!(
            get_memory(&ctx).unwrap(),
            "- likes short answers\n- has case #123\n"
        );
    }

    #[test]
    fn add_memory_creates_file_on_first_call() {
        let tmp = TempDir::new().unwrap();
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), SpecIndex::empty());
        let result = add_memory(&ctx, &json!({"fact": "prefers email over SMS"})).unwrap();
        assert_eq!(result, "recorded: prefers email over SMS");
        let contents = fs::read_to_string(tmp.path().join("memory.md")).unwrap();
        assert_eq!(contents, "- prefers email over SMS\n");
    }

    #[test]
    fn add_memory_appends_without_clobbering() {
        let tmp = TempDir::new().unwrap();
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), SpecIndex::empty());
        add_memory(&ctx, &json!({"fact": "first fact"})).unwrap();
        add_memory(&ctx, &json!({"fact": "second fact"})).unwrap();
        let contents = fs::read_to_string(tmp.path().join("memory.md")).unwrap();
        assert_eq!(contents, "- first fact\n- second fact\n");
    }

    #[test]
    fn add_memory_requires_fact_argument() {
        let tmp = TempDir::new().unwrap();
        let ctx = AppContext::for_test(tmp.path().to_path_buf(), SpecIndex::empty());
        let err = add_memory(&ctx, &json!({})).unwrap_err();
        assert!(err.contains("requires a 'fact'"));
    }

    #[test]
    fn tool_defs_returns_all_fourteen() {
        let defs = tool_defs();
        assert_eq!(defs.len(), TOOL_NAMES.len());
        for name in TOOL_NAMES {
            assert!(defs.iter().any(|d| d.name == *name), "missing def for {name}");
        }
    }
}
