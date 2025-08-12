use std::collections::HashMap;
use std::sync::Arc;

use codex_login::CodexAuth;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio::sync::futures::Notified;
use uuid::Uuid;

use crate::Codex;
use crate::CodexSpawnOk;
use crate::config::Config;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::Op;
use crate::protocol::SessionConfiguredEvent;
use crate::protocol::Submission;

/// Codex "server" that manages multiple conversations.
pub struct CodexServer {
    conversations: Arc<RwLock<HashMap<Uuid, Arc<CodexConversation>>>>,
}

/// Represents an active Codex conversation, including the first event
/// (which is [`EventMsg::SessionConfigured`]).
pub struct CodexConversation {
    codex: Codex,
    cancellation_token: Arc<Notify>,
}

impl CodexConversation {
    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        self.codex.submit(op).await
    }

    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.codex.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        self.codex.next_event().await
    }

    pub fn on_cancel(&self) -> Notified {
        self.cancellation_token.notified()
    }
}

pub struct NewConversation {
    pub conversation_id: Uuid,
    pub conversation: Arc<CodexConversation>,
    pub session_configured: SessionConfiguredEvent,
}

impl Default for CodexServer {
    fn default() -> Self {
        Self {
            conversations: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl CodexServer {
    pub async fn new_conversation(&self, config: Config) -> CodexResult<NewConversation> {
        // TODO(mbolin): Determine whether this should be wired up to SIGINT.
        let cancellation_token = Arc::new(Notify::new());
        let auth = CodexAuth::from_codex_home(&config.codex_home)?;

        let CodexSpawnOk {
            codex,
            init_id,
            session_id: conversation_id,
        } = Codex::spawn(config, auth, cancellation_token.clone()).await?;

        // The first event must be `SessionInitialized`. Validate and forward it
        // to the caller so that they can display it in the conversation
        // history.
        let event = codex.next_event().await?;
        let session_configured = match event {
            Event {
                id,
                msg: EventMsg::SessionConfigured(session_configured),
            } if id == init_id => session_configured,
            _ => {
                return Err(CodexErr::SessionConfiguredNotFirstEvent);
            }
        };

        let conversation = Arc::new(CodexConversation {
            codex,
            cancellation_token,
        });
        self.conversations
            .write()
            .await
            .insert(conversation_id, conversation.clone());

        Ok(NewConversation {
            conversation_id,
            conversation,
            session_configured,
        })
    }

    pub async fn get_conversation(
        &self,
        conversation_id: Uuid,
    ) -> CodexResult<Arc<CodexConversation>> {
        let conversations = self.conversations.read().await;
        conversations
            .get(&conversation_id)
            .cloned()
            .ok_or_else(|| CodexErr::ConversationNotFound(conversation_id))
    }
}
