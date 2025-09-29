use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;

#[derive(Debug, Clone)]
pub struct ProcessedResponseItem {
    pub item: ResponseItem,
    pub response: Option<ResponseInputItem>,
}

#[derive(Debug, Clone)]
pub struct TurnRunResult {
    pub processed_items: Vec<ProcessedResponseItem>,
    pub total_token_usage: Option<TokenUsage>,
}
