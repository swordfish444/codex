use crate::AuthManager;
use crate::CodexAuth;
use crate::codex::Codex;
use crate::codex::CodexSpawnOk;
use crate::codex::INITIAL_SUBMIT_ID;
use crate::codex_conversation::CodexConversation;
use crate::config::Config;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::event_mapping::map_response_item_to_event_messages;
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
    // Work from response items for cut logic; preserve any existing rollout items when possible.
    let rollout_items: Vec<RolloutItem> = history.get_rollout_items();
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

    let cut_events_index =
        find_matching_user_event_index_in_rollout(&rollout_items, &response_items, cut_resp_index);

    let rolled = build_truncated_rollout(rollout_items, cut_resp_index, cut_events_index);
    InitialHistory::Forked(rolled)
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

/// Derive the user message text (if any) associated with a response item using event mapping.
fn user_message_text_for_response(item: &ResponseItem) -> Option<String> {
    let mapped = map_response_item_to_event_messages(item, false);
    mapped.into_iter().find_map(|ev| match ev {
        EventMsg::UserMessage(u) => Some(u.message),
        _ => None,
    })
}

/// Given rollout items and the response-item cut index, locate the matching user EventMsg index.
fn find_matching_user_event_index_in_rollout(
    rollout_items: &[RolloutItem],
    response_items: &[ResponseItem],
    cut_resp_index: usize,
) -> Option<usize> {
    let target_message = user_message_text_for_response(&response_items[cut_resp_index])?;
    rollout_items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            RolloutItem::EventMsg(EventMsg::UserMessage(u)) if u.message == target_message => {
                Some(i)
            }
            _ => None,
        })
}

/// Build a truncated rollout keeping response items strictly before `cut_resp_index` and
/// event messages strictly before `event_cut_index` (when provided). Always keeps session meta.
fn build_truncated_rollout(
    rollout_items: Vec<RolloutItem>,
    cut_resp_index: usize,
    event_cut_index: Option<usize>,
) -> Vec<RolloutItem> {
    let mut kept_response_seen = 0usize;
    let mut rolled: Vec<RolloutItem> = Vec::new();
    for (abs_idx, it) in rollout_items.into_iter().enumerate() {
        match &it {
            RolloutItem::ResponseItem(_) => {
                if kept_response_seen < cut_resp_index {
                    rolled.push(it);
                }
                kept_response_seen += 1;
            }
            RolloutItem::EventMsg(_) => {
                if let Some(evt_cut) = event_cut_index
                    && abs_idx < evt_cut
                {
                    rolled.push(it);
                }
            }
            RolloutItem::SessionMeta(_) => {
                rolled.push(it);
            }
        }
    }
    rolled
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
