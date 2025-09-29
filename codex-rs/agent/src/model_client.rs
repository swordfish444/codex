use std::sync::Arc;

use async_trait::async_trait;
use codex_protocol::config_types::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;

use crate::client_common::Prompt;
use crate::client_common::ResponseStream;
use crate::model_family::ModelFamily;
use crate::model_provider::ModelProviderInfo;
use crate::services::CredentialsProvider;

#[async_trait]
pub trait ModelClientAdapter: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn get_model_context_window(&self) -> Option<u64>;

    fn get_auto_compact_token_limit(&self) -> Option<i64>;

    fn get_provider(&self) -> ModelProviderInfo;

    fn get_model(&self) -> String;

    fn get_model_family(&self) -> ModelFamily;

    fn get_reasoning_effort(&self) -> Option<ReasoningEffortConfig>;

    fn get_reasoning_summary(&self) -> ReasoningSummaryConfig;

    fn get_auth_manager(&self) -> Option<Arc<dyn CredentialsProvider>>;

    async fn stream(&self, prompt: &Prompt) -> Result<ResponseStream<Self::Error>, Self::Error>;
}
