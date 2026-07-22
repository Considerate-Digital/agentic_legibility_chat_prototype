//! MCP client machinery: per-server clients plus a router that multiplexes
//! them behind server-namespaced tool names.

pub mod legibility_chat_client;
pub mod router;
pub mod spec_tools;

pub use router::{namespaced_tools, McpClientEnum, McpRouter, ToolSource};
pub use spec_tools::{is_spec_tool, SPEC_TOOL_NAMES};
pub use legibility_chat_client::LegibilityChatClient;
// Re-export for `lib.rs` callers that name it explicitly.
#[allow(unused_imports)]
pub use router::McpServerHandle;
