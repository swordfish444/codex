//! Registry of model providers supported by Codex.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so Codex works out-of-the-box.
//!   2. User-defined entries inside `~/.codex/config.toml` under the `model_providers`
//!      key. These override or extend the defaults at runtime.

use async_trait::async_trait;
pub use codex_agent::ModelProviderInfo;
pub use codex_agent::WireApi;
use codex_protocol::mcp_protocol::AuthMode;
use std::collections::HashMap;
use std::env::VarError;
use std::sync::Arc;
use std::time::Duration;

use crate::CodexAuth;
use crate::ProviderAuth;
use crate::error::EnvVarError;

const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 5;
const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
/// Hard cap for user-configured `stream_max_retries`.
const MAX_STREAM_MAX_RETRIES: u64 = 100;
/// Hard cap for user-configured `request_max_retries`.
const MAX_REQUEST_MAX_RETRIES: u64 = 100;

#[async_trait]
pub trait ModelProviderExt {
    async fn create_request_builder(
        &self,
        client: &reqwest::Client,
        auth: &Option<Arc<dyn ProviderAuth>>,
    ) -> crate::error::Result<reqwest::RequestBuilder>;

    fn get_full_url(&self, auth: &Option<Arc<dyn ProviderAuth>>) -> String;

    fn is_azure_responses_endpoint(&self) -> bool;

    fn apply_http_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder;

    fn api_key(&self) -> crate::error::Result<Option<String>>;

    fn request_max_retries(&self) -> u64;

    fn stream_max_retries(&self) -> u64;

    fn stream_idle_timeout(&self) -> Duration;
}

#[async_trait]
impl ModelProviderExt for ModelProviderInfo {
    async fn create_request_builder(
        &self,
        client: &reqwest::Client,
        auth: &Option<Arc<dyn ProviderAuth>>,
    ) -> crate::error::Result<reqwest::RequestBuilder> {
        let effective_auth: Option<Arc<dyn ProviderAuth>> = match self.api_key()? {
            Some(key) => Some(Arc::new(CodexAuth::from_api_key(&key))),
            None => auth.clone(),
        };

        let url = self.get_full_url(&effective_auth);
        let mut builder = client.post(url);

        if let Some(auth) = effective_auth.as_ref() {
            builder = builder.bearer_auth(auth.access_token().await?);
        }

        Ok(self.apply_http_headers(builder))
    }

    fn get_full_url(&self, auth: &Option<Arc<dyn ProviderAuth>>) -> String {
        let default_base_url = if auth.as_ref().map(|a| a.mode()) == Some(AuthMode::ChatGPT) {
            "https://chatgpt.com/backend-api/codex"
        } else {
            "https://api.openai.com/v1"
        };
        let query_string = get_query_string(self);
        let base_url = self
            .base_url
            .clone()
            .unwrap_or(default_base_url.to_string());

        match self.wire_api {
            WireApi::Responses => format!("{base_url}/responses{query_string}"),
            WireApi::Chat => format!("{base_url}/chat/completions{query_string}"),
        }
    }

    fn is_azure_responses_endpoint(&self) -> bool {
        if self.wire_api != WireApi::Responses {
            return false;
        }

        if self.name.eq_ignore_ascii_case("azure") {
            return true;
        }

        self.base_url
            .as_ref()
            .map(|base| matches_azure_responses_base_url(base))
            .unwrap_or(false)
    }

    fn apply_http_headers(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(extra) = &self.http_headers {
            for (k, v) in extra {
                builder = builder.header(k, v);
            }
        }

        if let Some(env_headers) = &self.env_http_headers {
            for (header, env_var) in env_headers {
                if let Ok(val) = std::env::var(env_var)
                    && !val.trim().is_empty()
                {
                    builder = builder.header(header, val);
                }
            }
        }
        builder
    }

    fn api_key(&self) -> crate::error::Result<Option<String>> {
        match &self.env_key {
            Some(env_key) => {
                let env_value = std::env::var(env_key);
                env_value
                    .and_then(|v| {
                        if v.trim().is_empty() {
                            Err(VarError::NotPresent)
                        } else {
                            Ok(Some(v))
                        }
                    })
                    .map_err(|_| {
                        crate::error::CodexErr::EnvVar(EnvVarError {
                            var: env_key.clone(),
                            instructions: self.env_key_instructions.clone(),
                        })
                    })
            }
            None => Ok(None),
        }
    }

    fn request_max_retries(&self) -> u64 {
        self.request_max_retries
            .unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
            .min(MAX_REQUEST_MAX_RETRIES)
    }

    fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries
            .unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
            .min(MAX_STREAM_MAX_RETRIES)
    }

    fn stream_idle_timeout(&self) -> Duration {
        self.stream_idle_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS))
    }
}

fn get_query_string(provider: &ModelProviderInfo) -> String {
    provider
        .query_params
        .as_ref()
        .map_or_else(String::new, |params| {
            let full_params = params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");
            format!("?{full_params}")
        })
}

const DEFAULT_OLLAMA_PORT: u32 = 11434;

pub const BUILT_IN_OSS_MODEL_PROVIDER_ID: &str = "oss";

/// Built-in default provider list.
pub fn built_in_model_providers() -> HashMap<String, ModelProviderInfo> {
    use ModelProviderInfo as P;

    [
        (
            "openai",
            P {
                name: "OpenAI".into(),
                base_url: std::env::var("OPENAI_BASE_URL")
                    .ok()
                    .filter(|v| !v.trim().is_empty()),
                env_key: None,
                env_key_instructions: None,
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: Some(
                    [("version".to_string(), env!("CARGO_PKG_VERSION").to_string())]
                        .into_iter()
                        .collect(),
                ),
                env_http_headers: Some(
                    [
                        (
                            "OpenAI-Organization".to_string(),
                            "OPENAI_ORGANIZATION".to_string(),
                        ),
                        ("OpenAI-Project".to_string(), "OPENAI_PROJECT".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                request_max_retries: None,
                stream_max_retries: None,
                stream_idle_timeout_ms: None,
                requires_openai_auth: true,
            },
        ),
        (BUILT_IN_OSS_MODEL_PROVIDER_ID, create_oss_provider()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

pub fn create_oss_provider() -> ModelProviderInfo {
    let codex_oss_base_url = match std::env::var("CODEX_OSS_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        Some(url) => url,
        None => format!(
            "http://localhost:{port}/v1",
            port = std::env::var("CODEX_OSS_PORT")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(DEFAULT_OLLAMA_PORT)
        ),
    };

    create_oss_provider_with_base_url(&codex_oss_base_url)
}

pub fn create_oss_provider_with_base_url(base_url: &str) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "gpt-oss".into(),
        base_url: Some(base_url.into()),
        env_key: None,
        env_key_instructions: None,
        wire_api: WireApi::Chat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
    }
}

fn matches_azure_responses_base_url(base_url: &str) -> bool {
    let base = base_url.to_ascii_lowercase();
    const AZURE_MARKERS: [&str; 5] = [
        "openai.azure.",
        "cognitiveservices.azure.",
        "aoai.azure.",
        "azure-api.",
        "azurefd.",
    ];
    AZURE_MARKERS.iter().any(|marker| base.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn creates_request_builder_with_auth() {
        let provider = ModelProviderInfo {
            name: "openai".to_string(),
            base_url: None,
            env_key: None,
            env_key_instructions: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: true,
        };
        let client = reqwest::Client::new();
        let auth =
            Some(Arc::new(CodexAuth::create_dummy_chatgpt_auth_for_testing())
                as Arc<dyn ProviderAuth>);

        let builder = provider
            .create_request_builder(&client, &auth)
            .await
            .expect("builder");

        let request = builder.build().expect("request");
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(
            request.url().as_str(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[test]
    fn azure_detection() {
        let mut provider = create_oss_provider();
        assert!(!provider.is_azure_responses_endpoint());

        provider.name = "azure".to_string();
        provider.wire_api = WireApi::Responses;
        assert!(provider.is_azure_responses_endpoint());
    }
}
