use std::sync::Arc;

use tokio::sync::Notify;
use tokio::sync::futures::Notified;

use crate::codex::Codex;
use crate::error::Result as CodexResult;
use crate::protocol::Event;
use crate::protocol::Op;
use crate::protocol::Submission;

pub struct CodexConversation {
    codex: Codex,
    on_abort: Arc<Notify>,
}

impl CodexConversation {
    pub(crate) fn new(codex: Codex, on_abort: Arc<Notify>) -> Self {
        Self { codex, on_abort }
    }

    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        self.codex.submit(op).await
    }

    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.codex.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        self.codex.next_event().await
    }

    pub fn abort(&self) {
        self.on_abort.notify_waiters();
    }

    /// await this to get notified when the user _aborts_ the conversation.
    pub fn on_abort(&self) -> Notified {
        self.on_abort.notified()
    }
}
