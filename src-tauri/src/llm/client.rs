use anyhow::Result;
use tauri::AppHandle;

use crate::config::ProviderConfig;

use super::provider::Provider;
use super::types::{CompletionOutcome, LLMRequest};

/// Streaming completion — dispatches to the right provider module based on
/// the configured `base_url`. The rest of the codebase (notably `commands/chat`)
/// calls this without needing to know which provider is configured.
pub async fn stream_completion(
    config: &ProviderConfig,
    request: &LLMRequest,
    app: &AppHandle,
) -> Result<CompletionOutcome> {
    match Provider::detect(&config.base_url) {
        Provider::OpenAI => super::openai::stream(config, request, app).await,
        Provider::Anthropic => super::anthropic::stream(config, request, app).await,
    }
}

/// Non-streaming completion — dispatches by provider.
pub async fn complete(config: &ProviderConfig, request: &LLMRequest) -> Result<CompletionOutcome> {
    match Provider::detect(&config.base_url) {
        Provider::OpenAI => super::openai::complete(config, request).await,
        Provider::Anthropic => super::anthropic::complete(config, request).await,
    }
}