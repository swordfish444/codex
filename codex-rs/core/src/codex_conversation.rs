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
    cancellation_token: Arc<Notify>,
}

impl CodexConversation {
    pub(crate) fn new(codex: Codex, cancellation_token: Arc<Notify>) -> Self {
        Self {
            codex,
            cancellation_token,
        }
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

    /// await this to be get notified when the user cancels the conversation.
    pub fn on_cancel(&self) -> Notified {
        self.cancellation_token.notified()
    }
}
