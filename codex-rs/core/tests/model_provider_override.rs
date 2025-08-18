#![expect(clippy::unwrap_used)]

use std::collections::HashMap;

use codex_core::config::{Config, ConfigOverrides, ConfigToml};
use codex_core::model_provider_info::{ModelProviderInfo, WireApi};
use tempfile::TempDir;

#[test]
fn user_defined_provider_overrides_builtin() {
    let tmp = TempDir::new().unwrap();

    let mut cfg = ConfigToml::default();
    cfg.model_provider = Some("oss".to_string());
    cfg.model = Some("gpt-oss:20b".to_string());

    let mut providers = HashMap::new();
    providers.insert(
        "oss".to_string(),
        ModelProviderInfo {
            name: "Custom".into(),
            base_url: Some("https://example.com/v1".into()),
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
        },
    );
    cfg.model_providers = providers;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        tmp.path().to_path_buf(),
    )
    .unwrap();

    assert_eq!(config.model_provider.name, "Custom");
    assert_eq!(
        config.model_provider.base_url.as_deref(),
        Some("https://example.com/v1")
    );
}

