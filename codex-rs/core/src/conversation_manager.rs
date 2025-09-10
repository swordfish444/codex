use crate::AuthManager;
use crate::CodexAuth;
use crate::codex::Codex;
use crate::codex::CodexSpawnOk;
use crate::codex::INITIAL_SUBMIT_ID;
use crate::codex_conversation::CodexConversation;
use crate::config::Config;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::SessionConfiguredEvent;
use crate::rollout::RolloutRecorder;
use codex_protocol::mcp_protocol::ConversationId;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::InitialHistory;
use codex_protocol::protocol::RolloutItem;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents a newly created Codex conversation, including the first event
/// (which is [`EventMsg::SessionConfigured`]).
pub struct NewConversation {
    pub conversation_id: ConversationId,
    pub conversation: Arc<CodexConversation>,
    pub session_configured: SessionConfiguredEvent,
}

/// [`ConversationManager`] is responsible for creating conversations and
/// maintaining them in memory.
pub struct ConversationManager {
    conversations: Arc<RwLock<HashMap<ConversationId, Arc<CodexConversation>>>>,
    auth_manager: Arc<AuthManager>,
}

impl ConversationManager {
    pub fn new(auth_manager: Arc<AuthManager>) -> Self {
        Self {
            conversations: Arc::new(RwLock::new(HashMap::new())),
            auth_manager,
        }
    }

    /// Construct with a dummy AuthManager containing the provided CodexAuth.
    /// Used for integration tests: should not be used by ordinary business logic.
    pub fn with_auth(auth: CodexAuth) -> Self {
        Self::new(crate::AuthManager::from_auth_for_testing(auth))
    }

    pub async fn new_conversation(&self, config: Config) -> CodexResult<NewConversation> {
        self.spawn_conversation(config, self.auth_manager.clone())
            .await
    }

    async fn spawn_conversation(
        &self,
        config: Config,
        auth_manager: Arc<AuthManager>,
    ) -> CodexResult<NewConversation> {
        // TO BE REFACTORED: use the config experimental_resume field until we have a mainstream way.
        if let Some(resume_path) = config.experimental_resume.as_ref() {
            let initial_history = RolloutRecorder::get_rollout_history(resume_path).await?;
            let CodexSpawnOk {
                codex,
                conversation_id,
            } = Codex::spawn(config, auth_manager, initial_history).await?;
            self.finalize_spawn(codex, conversation_id).await
        } else {
            let CodexSpawnOk {
                codex,
                conversation_id,
            } = Codex::spawn(config, auth_manager, InitialHistory::New).await?;
            self.finalize_spawn(codex, conversation_id).await
        }
    }

    async fn finalize_spawn(
        &self,
        codex: Codex,
        conversation_id: ConversationId,
    ) -> CodexResult<NewConversation> {
        // The first event must be `SessionInitialized`. Validate and forward it
        // to the caller so that they can display it in the conversation
        // history.
        let event = codex.next_event().await?;
        let session_configured = match event {
            Event {
                id,
                msg: EventMsg::SessionConfigured(session_configured),
            } if id == INITIAL_SUBMIT_ID => session_configured,
            _ => {
                return Err(CodexErr::SessionConfiguredNotFirstEvent);
            }
        };

        let conversation = Arc::new(CodexConversation::new(codex));
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
        conversation_id: ConversationId,
    ) -> CodexResult<Arc<CodexConversation>> {
        let conversations = self.conversations.read().await;
        conversations
            .get(&conversation_id)
            .cloned()
            .ok_or_else(|| CodexErr::ConversationNotFound(conversation_id))
    }

    pub async fn resume_conversation_from_rollout(
        &self,
        config: Config,
        rollout_path: PathBuf,
        auth_manager: Arc<AuthManager>,
    ) -> CodexResult<NewConversation> {
        let initial_history = RolloutRecorder::get_rollout_history(&rollout_path).await?;
        let CodexSpawnOk {
            codex,
            conversation_id,
        } = Codex::spawn(config, auth_manager, initial_history).await?;
        self.finalize_spawn(codex, conversation_id).await
    }

    /// Removes the conversation from the manager's internal map, though the
    /// conversation is stored as `Arc<CodexConversation>`, it is possible that
    /// other references to it exist elsewhere. Returns the conversation if the
    /// conversation was found and removed.
    pub async fn remove_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Option<Arc<CodexConversation>> {
        self.conversations.write().await.remove(conversation_id)
    }

    /// Fork an existing conversation by dropping the last `drop_last_messages`
    /// user/assistant messages from its transcript and starting a new
    /// conversation with identical configuration (unless overridden by the
    /// caller's `config`). The new conversation will have a fresh id.
    pub async fn fork_conversation(
        &self,
        conversation_history: InitialHistory,
        num_messages_to_drop: usize,
        config: Config,
    ) -> CodexResult<NewConversation> {
        // Compute the prefix up to the cut point.
        let history =
            truncate_after_dropping_last_messages(conversation_history, num_messages_to_drop);

        // Spawn a new conversation with the computed initial history.
        let auth_manager = self.auth_manager.clone();
        let CodexSpawnOk {
            codex,
            conversation_id,
        } = Codex::spawn(config, auth_manager, history).await?;

        self.finalize_spawn(codex, conversation_id).await
    }
}

/// Return a prefix of `items` obtained by dropping the last `n` user messages
/// and all items that follow them.
fn truncate_after_dropping_last_messages(history: InitialHistory, n: usize) -> InitialHistory {
    // Determine the cut point among response items (counting only ResponseItem::Message with role=="user").
    let response_items: Vec<ResponseItem> = history.get_response_items();
    if n == 0 {
        return history;
    }

    let Some(cut_resp_index) = find_cut_response_index(&response_items, n) else {
        return InitialHistory::New;
    };

    if cut_resp_index == 0 {
        return InitialHistory::New;
    }

    // Compute event prefix by dropping the last `n` user events (counted from the end).
    let event_msgs_prefix: Vec<EventMsg> =
        event_msgs_prefix_after_dropping_last_user_events(&history, n);

    // Keep only response items strictly before the cut response index.
    let response_prefix: Vec<ResponseItem> = response_items[..cut_resp_index].to_vec();

    let rolled = build_truncated_rollout(&event_msgs_prefix, &response_prefix);
    InitialHistory::Forked(rolled)
}

/// Build the event messages prefix from `history` by dropping the last `n` user
/// events (counted from the end) and taking everything before that cut.
fn event_msgs_prefix_after_dropping_last_user_events(
    history: &InitialHistory,
    n: usize,
) -> Vec<EventMsg> {
    match history.get_event_msgs() {
        Some(all_events) => {
            if let Some(idx) = find_cut_event_index(&all_events, n) {
                all_events[..idx].to_vec()
            } else {
                Vec::new()
            }
        }
        None => Vec::new(),
    }
}

/// Find the index (into response items) of the Nth user message from the end.
fn find_cut_response_index(response_items: &[ResponseItem], n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let mut remaining = n;
    for (idx, item) in response_items.iter().enumerate().rev() {
        if let ResponseItem::Message { role, .. } = item
            && role == "user"
        {
            remaining -= 1;
            if remaining == 0 {
                return Some(idx);
            }
        }
    }
    None
}

/// Find the index (into event messages) of the Nth user event from the end.
fn find_cut_event_index(event_msgs: &[EventMsg], n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let mut remaining = n;
    for (idx, ev) in event_msgs.iter().enumerate().rev() {
        if matches!(ev, EventMsg::UserMessage(_)) {
            remaining -= 1;
            if remaining == 0 {
                return Some(idx);
            }
        }
    }
    None
}

/// Build a truncated rollout by concatenating the (already-sliced) event messages and response items.
fn build_truncated_rollout(
    event_msgs: &[EventMsg],
    response_items: &[ResponseItem],
) -> Vec<RolloutItem> {
    let mut rolled: Vec<RolloutItem> = Vec::with_capacity(event_msgs.len() + response_items.len());
    rolled.extend(event_msgs.iter().cloned().map(RolloutItem::EventMsg));
    rolled.extend(
        response_items
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem),
    );
    rolled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_mapping::map_response_item_to_event_messages;
    use crate::protocol::EventMsg;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ReasoningItemReasoningSummary;
    use codex_protocol::models::ResponseItem;

    fn user_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }
    fn user_input(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
        }
    }
    fn assistant_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn drops_from_last_user_only() {
        let items = vec![
            user_msg("u1"),
            assistant_msg("a1"),
            assistant_msg("a2"),
            user_msg("u2"),
            assistant_msg("a3"),
            ResponseItem::Reasoning {
                id: "r1".to_string(),
                summary: vec![ReasoningItemReasoningSummary::SummaryText {
                    text: "s".to_string(),
                }],
                content: None,
                encrypted_content: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "tool".to_string(),
                arguments: "{}".to_string(),
                call_id: "c1".to_string(),
            },
            assistant_msg("a4"),
        ];

        // Wrap as InitialHistory::Forked with response items only.
        let initial: Vec<RolloutItem> = items
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem)
            .collect();
        let truncated = truncate_after_dropping_last_messages(InitialHistory::Forked(initial), 1);
        let got_items = truncated.get_rollout_items();
        let expected_items = vec![
            RolloutItem::ResponseItem(items[0].clone()),
            RolloutItem::ResponseItem(items[1].clone()),
            RolloutItem::ResponseItem(items[2].clone()),
        ];
        assert_eq!(
            serde_json::to_value(&got_items).unwrap(),
            serde_json::to_value(&expected_items).unwrap()
        );

        let initial2: Vec<RolloutItem> = items
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem)
            .collect();
        let truncated2 = truncate_after_dropping_last_messages(InitialHistory::Forked(initial2), 2);
        assert!(matches!(truncated2, InitialHistory::New));
    }

    #[test]
    fn event_prefix_counts_from_end_with_duplicate_user_prompts() {
        // Two identical user prompts with assistant replies between them.
        let responses = vec![
            user_input("same"),
            assistant_msg("a1"),
            user_input("same"),
            assistant_msg("a2"),
        ];

        // Derive event messages in order from responses (user → UserMessage, assistant → AgentMessage).
        let mut events: Vec<EventMsg> = Vec::new();
        for r in &responses {
            events.extend(map_response_item_to_event_messages(r, false));
        }

        // Build initial history containing both events and responses.
        let mut initial: Vec<RolloutItem> = Vec::new();
        initial.extend(events.iter().cloned().map(RolloutItem::EventMsg));
        initial.extend(responses.iter().cloned().map(RolloutItem::ResponseItem));

        // Drop the last user turn.
        let truncated = truncate_after_dropping_last_messages(InitialHistory::Forked(initial), 1);

        // Expect the event prefix to include the first user + first assistant only,
        // and the response prefix to include the first user + first assistant only.
        let got_items = truncated.get_rollout_items();

        // Compute expected events and responses after cut.
        let expected_event_prefix: Vec<RolloutItem> = events[..2]
            .iter()
            .cloned()
            .map(RolloutItem::EventMsg)
            .collect();
        let expected_response_prefix: Vec<RolloutItem> = responses[..2]
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem)
            .collect();

        let mut expected: Vec<RolloutItem> = Vec::new();
        expected.extend(expected_event_prefix);
        expected.extend(expected_response_prefix);

        assert_eq!(
            serde_json::to_value(&got_items).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
}
