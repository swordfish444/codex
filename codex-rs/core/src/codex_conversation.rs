use crate::codex::Codex;
use crate::error::Result as CodexResult;
use crate::models::ResponseItem;
use crate::protocol::Event;
use crate::protocol::Op;
use crate::protocol::Submission;

pub struct CodexConversation {
    codex: Codex,
}

/// Conduit for the bidirectional stream of messages that compose a conversation
/// in Codex.
impl CodexConversation {
    pub(crate) fn new(codex: Codex) -> Self {
        Self { codex }
    }

    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        self.codex.submit(op).await
    }

    /// Use sparingly: this is intended to be removed soon.
    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.codex.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        self.codex.next_event().await
    }

    /// Return a snapshot of the current conversation history (oldest → newest).
    pub(crate) fn history_contents(&self) -> Vec<ResponseItem> {
        self.codex.history_contents()
    }
}
