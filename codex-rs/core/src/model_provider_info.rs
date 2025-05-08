//! Registry of model providers supported by Codex.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so Codex works out-of-the-box.
//!   2. User-defined entries inside `~/.codex/config.toml` under the `model_providers`
//!      key. These override or extend the defaults at runtime.

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// Wire protocol that the provider speaks. Most third-party services only
/// implement the classic OpenAI Chat Completions JSON schema, whereas OpenAI
/// itself (and a handful of others) additionally expose the more modern
/// *Responses* API.  The two protocols use different request/response shapes
/// and *cannot* be auto-detected at runtime, therefore each provider entry
/// must declare which one it expects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    /// The experimental “Responses” API exposed by OpenAI at `/v1/responses`.
    #[default]
    Responses,
    /// Regular Chat Completions compatible with `/v1/chat/completions`.
    Chat,
}

/// Serializable representation of a provider definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelProviderInfo {
    /// Friendly display name.
    pub name: String,
    /// Base URL for the provider's OpenAI-compatible API.
    pub base_url: String,
    /// Environment variable that stores the user's API key for this provider.
    pub env_key: String,

    /// Which wire protocol this provider expects.  Defaults to
    /// `WireApi::Responses` to keep backward-compatibility with existing user
    /// configs.
    #[serde(default)]
    pub wire_api: WireApi,
}

impl ModelProviderInfo {
    /// Returns the API key for this provider if present in the environment.
    pub fn api_key(&self) -> crate::error::Result<String> {
        std::env::var(&self.env_key)
            .map_err(|_| crate::error::CodexErr::EnvVar(self.env_key.clone()))
    }
}

/// Built-in default provider list.
pub fn built_in_model_providers() -> HashMap<String, ModelProviderInfo> {
    use ModelProviderInfo as P;

    [
        (
            "openai",
            P {
                name: "OpenAI".into(),
                base_url: "https://api.openai.com/v1".into(),
                env_key: "OPENAI_API_KEY".into(),
                wire_api: WireApi::Responses,
            },
        ),
        (
            "openrouter",
            P {
                name: "OpenRouter".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                env_key: "OPENROUTER_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "gemini",
            P {
                name: "Gemini".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                env_key: "GEMINI_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "ollama",
            P {
                name: "Ollama".into(),
                base_url: "http://localhost:11434/v1".into(),
                env_key: "OLLAMA_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "mistral",
            P {
                name: "Mistral".into(),
                base_url: "https://api.mistral.ai/v1".into(),
                env_key: "MISTRAL_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "deepseek",
            P {
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com".into(),
                env_key: "DEEPSEEK_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "xai",
            P {
                name: "xAI".into(),
                base_url: "https://api.x.ai/v1".into(),
                env_key: "XAI_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
        (
            "groq",
            P {
                name: "Groq".into(),
                base_url: "https://api.groq.com/openai/v1".into(),
                env_key: "GROQ_API_KEY".into(),
                wire_api: WireApi::Chat,
            },
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}
