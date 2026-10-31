//! LLM providers: local (llama.cpp) and any OpenAI-compatible endpoint
//! (OpenAI, OpenCode Zen/Go, OpenRouter, Ollama, LM Studio...).

pub mod config;
pub mod history;
pub mod openai_compat;
pub mod presets;
pub mod prompt;
pub mod secrets;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Which wire format an endpoint speaks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiStyle {
    ChatCompletions,
    Responses,
    AnthropicMessages,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub api_style: ApiStyle,
    pub model: Option<String>,
    /// Reference to the secret, e.g. `keychain:openai` or `env:OPENAI_API_KEY`.
    /// The key itself is never stored in config.
    pub key_ref: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("missing API key for provider {0}")]
    MissingKey(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("request failed: {0}")]
    Request(String),
    /// The OS keychain, or an environment variable, could not be read or written.
    /// The message never contains the key.
    #[error("key storage error: {0}")]
    Keychain(String),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn is_local(&self) -> bool;
    async fn list_models(&self) -> Result<Vec<String>, LlmError>;
    async fn test_connection(&self) -> Result<(), LlmError>;
    /// Streams tokens through `on_token`; returns the full answer.
    async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        // Spelled out because `async_trait` would otherwise tie the `&str` to one lifetime.
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> Result<String, LlmError>;
}
