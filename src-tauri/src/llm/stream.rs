/// Provider-agnostic events extracted from a single SSE block
/// (the text between two `\n\n` separators in an SSE byte stream).
///
/// The main loop in `client::stream_completion` matches on these and applies
/// them to the conversation accumulators, without needing to know which
/// provider produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawEvent {
    /// A piece of assistant text content.
    Text(String),
    /// A delta on a tool call being built up across multiple events.
    ///
    /// `index` keys the tool call within a single response — each tool call
    /// has its own index that the main loop uses to accumulate across
    /// deltas. `id`, `name`, and `arguments_fragment` are all optional
    /// because different providers deliver them in different shapes:
    /// OpenAI streams them across deltas; Anthropic delivers `id` and
    /// `name` once on `content_block_start` and only streams `arguments`.
    ToolCall {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_fragment: Option<String>,
    },
    /// The model finished; the wrapped string is the provider's
    /// finish/stop reason (e.g. `stop`, `tool_use`, `end_turn`, `length`).
    Finish(String),
}