use async_trait::async_trait;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::Submission;

pub mod turn;

pub use turn::ProcessedResponseItem;
pub use turn::TurnRunResult;

/// Minimal async interface for interacting with an agent runtime.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn submit(&self, op: Op) -> Result<String, Self::Error>;

    async fn submit_with_id(&self, submission: Submission) -> Result<(), Self::Error>;

    async fn next_event(&self) -> Result<Event, Self::Error>;
}
