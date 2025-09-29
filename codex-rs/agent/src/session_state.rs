use std::collections::HashSet;

use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::RateLimitSnapshot;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TokenUsageInfo;

use crate::conversation_history::ConversationHistory;

/// Persistent, session-scoped state previously stored directly on `Session`.
#[derive(Default)]
pub struct SessionState {
    approved_commands: HashSet<Vec<String>>,
    history: ConversationHistory,
    token_info: Option<TokenUsageInfo>,
    latest_rate_limits: Option<RateLimitSnapshot>,
}

impl SessionState {
    /// Create a new session state mirroring previous `State::default()` semantics.
    pub fn new() -> Self {
        Self {
            history: ConversationHistory::new(),
            ..Default::default()
        }
    }

    // History helpers
    pub fn record_items<I>(&mut self, items: I)
    where
        I: IntoIterator,
        I::Item: std::ops::Deref<Target = ResponseItem>,
    {
        self.history.record_items(items)
    }

    pub fn history_snapshot(&self) -> Vec<ResponseItem> {
        self.history.contents()
    }

    pub fn replace_history(&mut self, items: Vec<ResponseItem>) {
        self.history.replace(items);
    }

    // Approved command helpers
    pub fn add_approved_command(&mut self, cmd: Vec<String>) {
        self.approved_commands.insert(cmd);
    }

    pub fn approved_commands_ref(&self) -> &HashSet<Vec<String>> {
        &self.approved_commands
    }

    // Token/rate limit helpers
    pub fn update_token_info_from_usage(
        &mut self,
        usage: &TokenUsage,
        model_context_window: Option<u64>,
    ) {
        self.token_info = TokenUsageInfo::new_or_append(
            &self.token_info,
            &Some(usage.clone()),
            model_context_window,
        );
    }

    pub fn set_rate_limits(&mut self, snapshot: RateLimitSnapshot) {
        self.latest_rate_limits = Some(snapshot);
    }

    pub fn token_info_and_rate_limits(
        &self,
    ) -> (Option<TokenUsageInfo>, Option<RateLimitSnapshot>) {
        (self.token_info.clone(), self.latest_rate_limits.clone())
    }
}
