//! Hardcoded list of tool names exposed by the spec-lookup tools natively
//! ported into `legibility-chat-mcp` (formerly served by the separate
//! `legibility-mcp` HTTP sidecar).
//!
//! These tool names are baked into `legibility-chat-mcp` at compile time and are
//! only registered when `LIVE_RESOURCES_DIR` is set (see `src-mcp/src/tools/mod.rs`,
//! `all_tool_defs`). The Tauri app fetches the live list at startup via
//! `tools/list`, but we also keep a hardcoded list here so we can recognise
//! "the LLM tried to call a spec tool" *before* the server has confirmed it
//! (or when `live_resources_dir` is unset and the sidecar never registered
//! these tools).
//!
//! When `legibility-chat-mcp` gains new spec tools, add them here too. New tools
//! that aren't in this list will fall through to the generic
//! "no MCP server owns tool" error message — they'll still work at
//! runtime (if spec tools are configured), but they won't get the
//! "Open Setup from the top bar to configure it." suggestion. This is a
//! deliberate trade-off: a stale-but-useful list
//! beats a runtime call we can't make (the tool isn't registered yet, so
//! we can't ask it).

/// Every spec-lookup tool name gated on `LIVE_RESOURCES_DIR` in `legibility-chat-mcp`.
///
/// Keep this list in sync with `src-mcp/src/tools/mod.rs`.
pub const SPEC_TOOL_NAMES: &[&str] = &[
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

/// True if `name` is a spec-lookup tool gated on `LIVE_RESOURCES_DIR`.
/// O(n) but n=14 so the constant lookup is faster than a HashMap for this size.
pub fn is_spec_tool(name: &str) -> bool {
    SPEC_TOOL_NAMES.contains(&name)
}
