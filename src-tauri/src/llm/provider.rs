use serde::{Deserialize, Serialize};

/// Identifies which LLM provider we're talking to.
///
/// OpenRouter's `/chat/completions` is fully OpenAI-compatible, so it shares
/// the `OpenAI` variant — no provider-specific code is needed for it.
/// Anthropic uses a different protocol on `/v1/messages`, so it has its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Any OpenAI-compatible endpoint that accepts `POST {root}/chat/completions`
    /// with `{model, messages, tools, …}` and returns SSE with
    /// `choices[].delta` chunks. Covers OpenAI, Together, Groq, Ollama,
    /// LM Studio, custom proxies, and OpenRouter's `/chat/completions`.
    OpenAI,
    /// Anthropic's native `POST {root}/v1/messages` endpoint with `x-api-key`
    /// auth, a separate top-level `system` field, flat tool definitions
    /// (`{name, description, input_schema}`), and Anthropic-style SSE events
    /// (`content_block_delta`, `message_delta`, …).
    Anthropic,
}

impl Provider {
    /// Detect the provider from a configured base URL.
    ///
    /// Detection is intentionally URL-based and conservative: anything that
    /// contains `anthropic.com` is Anthropic; everything else falls through
    /// to the OpenAI-compatible default. This makes the OpenAI variant the
    /// "safe default" for any third-party OpenAI-compatible proxy a user
    /// might be routing through.
    pub fn detect(base_url: &str) -> Self {
        let lower = base_url.to_lowercase();
        if lower.contains("anthropic.com") {
            Self::Anthropic
        } else {
            Self::OpenAI
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_anthropic() {
        assert_eq!(Provider::detect("https://api.anthropic.com"), Provider::Anthropic);
        assert_eq!(Provider::detect("https://api.anthropic.com/v1"), Provider::Anthropic);
        assert_eq!(Provider::detect("https://api.ANTHROPIC.com/v1"), Provider::Anthropic);
        assert_eq!(
            Provider::detect("https://my-proxy.com/anthropic.com/v1"),
            Provider::Anthropic,
        );
    }

    #[test]
    fn detect_openai_default() {
        assert_eq!(Provider::detect("https://api.openai.com/v1"), Provider::OpenAI);
        assert_eq!(Provider::detect("https://openrouter.ai/api/v1"), Provider::OpenAI);
        assert_eq!(Provider::detect("https://api.together.xyz/v1"), Provider::OpenAI);
        assert_eq!(Provider::detect("https://api.groq.com/openai/v1"), Provider::OpenAI);
        // Empty / malformed → default to OpenAI-compatible.
        assert_eq!(Provider::detect(""), Provider::OpenAI);
    }
}