use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;

use codex_utils_readiness::ReadinessFlag;
use tokio::sync::oneshot;

use crate::conversation_history::ConversationHistory;
use crate::protocol::RateLimitSnapshot;
use crate::protocol::ReviewDecision;
use crate::protocol::TokenUsageInfo;

pub(crate) struct SessionState {
    pub(crate) approved_commands: HashSet<Vec<String>>,
    pub(crate) pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pub(crate) history: ConversationHistory,
    pub(crate) token_info: Option<TokenUsageInfo>,
    pub(crate) latest_rate_limits: Option<RateLimitSnapshot>,
    readiness_queue: VecDeque<Arc<ReadinessFlag>>,
}

impl SessionState {
    pub(crate) fn push_readiness(&mut self, flag: Arc<ReadinessFlag>) {
        self.readiness_queue.push_back(flag);
    }

    pub(crate) fn next_readiness(&mut self) -> Option<Arc<ReadinessFlag>> {
        self.readiness_queue.pop_front()
    }

    pub(crate) fn clear_readiness(&mut self) {
        self.readiness_queue.clear();
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            approved_commands: HashSet::new(),
            pending_approvals: HashMap::new(),
            history: ConversationHistory::new(),
            token_info: None,
            latest_rate_limits: None,
            readiness_queue: VecDeque::new(),
        }
    }
}
