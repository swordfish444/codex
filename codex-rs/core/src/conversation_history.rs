use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::RolloutItem;

/// Transcript of conversation history
#[derive(Debug, Clone, Default)]
pub(crate) struct ResponseItemsHistory {
    /// The oldest items are at the beginning of the vector.
    items: Vec<ResponseItem>,
}

impl ResponseItemsHistory {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Returns a clone of the contents in the transcript.
    pub(crate) fn contents(&self) -> Vec<ResponseItem> {
        self.items.clone()
    }

    /// `items` is ordered from oldest to newest.
    pub(crate) fn record_items<I>(&mut self, items: I)
    where
        I: IntoIterator,
        I::Item: std::ops::Deref<Target = ResponseItem>,
    {
        for item in items {
            if !is_api_message(&item) {
                continue;
            }

            self.items.push(item.clone());
        }
    }

    pub(crate) fn keep_last_messages(&mut self, n: usize) {
        if n == 0 {
            self.items.clear();
            return;
        }

        // Collect the last N message items (assistant/user), newest to oldest.
        let mut kept: Vec<ResponseItem> = Vec::with_capacity(n);
        for item in self.items.iter().rev() {
            if let ResponseItem::Message { role, content, .. } = item {
                kept.push(ResponseItem::Message {
                    // we need to remove the id or the model will complain that messages are sent without
                    // their reasonings
                    id: None,
                    role: role.clone(),
                    content: content.clone(),
                });
                if kept.len() == n {
                    break;
                }
            }
        }

        // Preserve chronological order (oldest to newest) within the kept slice.
        kept.reverse();
        self.items = kept;
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EventMsgsHistory {
    items: Vec<EventMsg>,
}

impl EventMsgsHistory {
    pub(crate) fn record_items<I>(&mut self, items: I)
    where
        I: IntoIterator,
        I::Item: std::ops::Deref<Target = EventMsg>,
    {
        for item in items {
            if self.should_record_item(&item) {
                self.items.push(item.clone());
            }
        }
    }
    fn should_record_item(&self, item: &EventMsg) -> bool {
        !matches!(
            item,
            EventMsg::AgentMessageDelta(_)
                | EventMsg::AgentReasoningDelta(_)
                | EventMsg::AgentReasoningRawContentDelta(_)
                | EventMsg::ExecCommandOutputDelta(_)
        )
    }
}

impl From<&ResponseItemsHistory> for Vec<RolloutItem> {
    fn from(history: &ResponseItemsHistory) -> Self {
        history
            .items
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem)
            .collect()
    }
}

impl From<&EventMsgsHistory> for Vec<RolloutItem> {
    fn from(history: &EventMsgsHistory) -> Self {
        history
            .items
            .iter()
            .cloned()
            .map(RolloutItem::EventMsg)
            .collect()
    }
}

/// Anything that is not a system message or "reasoning" message is considered
/// an API message.
fn is_api_message(message: &ResponseItem) -> bool {
    match message {
        ResponseItem::Message { role, .. } => role.as_str() != "system",
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::Reasoning { .. } => true,
        ResponseItem::WebSearchCall { .. } | ResponseItem::Other => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::ContentItem;

    fn assistant_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    fn user_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn filters_non_api_messages() {
        let mut h = ResponseItemsHistory::default();
        // System message is not an API message; Other is ignored.
        let system = ResponseItem::Message {
            id: None,
            role: "system".to_string(),
            content: vec![ContentItem::OutputText {
                text: "ignored".to_string(),
            }],
        };
        h.record_items([&system, &ResponseItem::Other]);

        // User and assistant should be retained.
        let u = user_msg("hi");
        let a = assistant_msg("hello");
        h.record_items([&u, &a]);

        let items = h.contents();
        assert_eq!(
            items,
            vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "hi".to_string()
                    }]
                },
                ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "hello".to_string()
                    }]
                }
            ]
        );
    }
}
