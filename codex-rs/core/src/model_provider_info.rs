//! Registry of model providers supported by Codex.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so Codex works out-of-the-box.
//!   2. User-defined entries inside `~/.codex/config.toml` under the `providers`
//!      key.  These override or extend the defaults at runtime.
//!
//! The combined mapping is surfaced via [`provider_map()`] and used by helper
//! functions in [`crate::flags`] to resolve API keys and base URLs.

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// Serializable representation of a provider definition.
///
/// All fields are owned `String`s so that user-defined providers loaded from
/// disk can be stored alongside the built-ins without lifetime headaches.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelProviderInfo {
    /// Friendly display name (optional for built-ins).
    #[serde(default)]
    pub name: String,
    /// Base URL for the provider’s OpenAI-compatible API.
    pub base_url: String,
    /// Environment variable that stores the user’s API key for this provider.
    pub env_key: String,
}

impl ModelProviderInfo {
    /// Returns the API key for this provider if present in the environment.
    pub fn api_key(&self) -> Option<String> {
        std::env::var(&self.env_key).ok()
    }

    /// Determines the base URL for API requests, giving precedence to the
    /// `{{PROVIDER}}_BASE_URL` environment variable when it is set.
    pub fn api_base(&self, provider_key: &str) -> String {
        let override_key = format!("{}_BASE_URL", provider_key.to_uppercase());
        if let Ok(val) = std::env::var(&override_key) {
            if !val.is_empty() {
                return val;
            }
        }
        self.base_url.clone()
    }
}

/// Built-in default provider list – mirrors `codex-cli/src/utils/providers.ts`.
/// Built-in provider registry. Public so callers (e.g. flags.rs) can resolve
/// information without needing a full [`crate::config::Config`].
pub fn built_in_model_providers() -> HashMap<String, ModelProviderInfo> {
    use ModelProviderInfo as P;

    [
        (
            "openai",
            P {
                name: "OpenAI".into(),
                base_url: "https://api.openai.com/v1".into(),
                env_key: "OPENAI_API_KEY".into(),
            },
        ),
        (
            "openrouter",
            P {
                name: "OpenRouter".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                env_key: "OPENROUTER_API_KEY".into(),
            },
        ),
        (
            "gemini",
            P {
                name: "Gemini".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                env_key: "GEMINI_API_KEY".into(),
            },
        ),
        (
            "ollama",
            P {
                name: "Ollama".into(),
                base_url: "http://localhost:11434/v1".into(),
                env_key: "OLLAMA_API_KEY".into(),
            },
        ),
        (
            "mistral",
            P {
                name: "Mistral".into(),
                base_url: "https://api.mistral.ai/v1".into(),
                env_key: "MISTRAL_API_KEY".into(),
            },
        ),
        (
            "deepseek",
            P {
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com".into(),
                env_key: "DEEPSEEK_API_KEY".into(),
            },
        ),
        (
            "xai",
            P {
                name: "xAI".into(),
                base_url: "https://api.x.ai/v1".into(),
                env_key: "XAI_API_KEY".into(),
            },
        ),
        (
            "groq",
            P {
                name: "Groq".into(),
                base_url: "https://api.groq.com/openai/v1".into(),
                env_key: "GROQ_API_KEY".into(),
            },
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}
