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

#[derive(Default)]
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

    pub(crate) fn next_readiness(&mut self) -> Option<TurnReadinessGuard<'_>> {
        if self.readiness_queue.is_empty() {
            None
        } else {
            Some(TurnReadinessGuard::new(&mut self.readiness_queue))
        }
    }

    pub(crate) fn clear_readiness(&mut self) {
        self.readiness_queue.clear();
    }
}

pub(crate) struct TurnReadinessGuard<'a> {
    queue: &'a mut VecDeque<Arc<ReadinessFlag>>,
    consumed: bool,
}

impl<'a> TurnReadinessGuard<'a> {
    fn new(queue: &'a mut VecDeque<Arc<ReadinessFlag>>) -> Self {
        Self {
            queue,
            consumed: false,
        }
    }

    pub(crate) fn take(mut self) -> Option<Arc<ReadinessFlag>> {
        self.consumed = true;
        self.queue.pop_front()
    }
}

impl Drop for TurnReadinessGuard<'_> {
    fn drop(&mut self) {
        if !self.consumed {
            let _ = self.queue.pop_front();
        }
    }
}
