use std::collections::HashMap;

use codex_protocol::mcp_protocol::AuthMode;
use serde::Deserialize;
use serde::Serialize;

/// Wire protocol variants supported by model providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    Responses,
    #[default]
    Chat,
}

/// Serializable representation of a provider definition shared across hosts.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ModelProviderInfo {
    pub name: String,
    pub base_url: Option<String>,
    pub env_key: Option<String>,
    pub env_key_instructions: Option<String>,
    #[serde(default)]
    pub wire_api: WireApi,
    pub query_params: Option<HashMap<String, String>>,
    pub http_headers: Option<HashMap<String, String>>,
    pub env_http_headers: Option<HashMap<String, String>>,
    pub request_max_retries: Option<u64>,
    pub stream_max_retries: Option<u64>,
    pub stream_idle_timeout_ms: Option<u64>,
    #[serde(default)]
    pub requires_openai_auth: bool,
}

impl ModelProviderInfo {
    pub fn wire_api(&self) -> WireApi {
        self.wire_api
    }

    pub fn requires_auth(&self) -> bool {
        self.requires_openai_auth
    }

    pub fn base_url(&self, auth_mode: AuthMode) -> String {
        let fallback = if auth_mode == AuthMode::ChatGPT {
            "https://chatgpt.com/backend-api/codex"
        } else {
            "https://api.openai.com/v1"
        };
        self.base_url
            .clone()
            .unwrap_or_else(|| fallback.to_string())
    }
}
