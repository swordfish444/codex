use std::sync::Arc;

use async_trait::async_trait;

use crate::client::ModelClient;
use crate::client_common::Prompt;
use crate::error::CodexErr;
use crate::model_family::ModelFamily;
use codex_agent::model_client::ModelClientAdapter;
use codex_agent::model_provider::ModelProviderInfo;
use codex_agent::services::CredentialsProvider;

#[derive(Clone)]
pub struct CoreModelClientAdapter {
    inner: ModelClient,
}

impl CoreModelClientAdapter {
    pub fn new(inner: ModelClient) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &ModelClient {
        &self.inner
    }
}

#[async_trait]
impl ModelClientAdapter for CoreModelClientAdapter {
    type Error = CodexErr;

    fn get_model_context_window(&self) -> Option<u64> {
        self.inner.get_model_context_window()
    }

    fn get_auto_compact_token_limit(&self) -> Option<i64> {
        self.inner.get_auto_compact_token_limit()
    }

    fn get_provider(&self) -> ModelProviderInfo {
        self.inner.get_provider()
    }

    fn get_model(&self) -> String {
        self.inner.get_model()
    }

    fn get_model_family(&self) -> ModelFamily {
        self.inner.get_model_family()
    }

    fn get_reasoning_effort(&self) -> Option<codex_protocol::config_types::ReasoningEffort> {
        self.inner.get_reasoning_effort()
    }

    fn get_reasoning_summary(&self) -> codex_protocol::config_types::ReasoningSummary {
        self.inner.get_reasoning_summary()
    }

    fn get_auth_manager(&self) -> Option<Arc<dyn CredentialsProvider>> {
        self.inner.get_auth_manager()
    }

    async fn stream(
        &self,
        prompt: &Prompt,
    ) -> Result<codex_agent::client_common::ResponseStream<Self::Error>, Self::Error> {
        self.inner.stream(prompt).await
    }
}
