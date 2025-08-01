use crate::models::ResponseItem;

/// Transcript of conversation history
#[derive(Debug, Clone, Default)]
pub(crate) struct ConversationHistory {
    /// The oldest items are at the beginning of the vector.
    items: Vec<ResponseItem>,
}

impl ConversationHistory {
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

            // Merge adjacent assistant messages into a single history entry.
            // This prevents duplicates when a partial assistant message was
            // streamed into history earlier in the turn and the final full
            // message is recorded at turn end.
            match (&*item, self.items.last_mut()) {
                (
                    ResponseItem::Message {
                        role: new_role,
                        content: new_content,
                        ..
                    },
                    Some(ResponseItem::Message {
                        role: last_role,
                        content: last_content,
                        ..
                    }),
                ) if new_role == "assistant" && last_role == "assistant" => {
                    append_text_content(last_content, new_content);
                }
                _ => {
                    // Note agent-loop.ts also does filtering on some of the fields.
                    self.items.push(item.clone());
                }
            }
        }
    }

    /// Append a text `delta` to the latest assistant message, creating a new
    /// assistant entry if none exists yet (e.g. first delta for this turn).
    pub(crate) fn append_assistant_text(&mut self, delta: &str) {
        match self.items.last_mut() {
            Some(ResponseItem::Message { role, content, .. }) if role == "assistant" => {
                append_text_delta(content, delta);
            }
            _ => {
                // Start a new assistant message with the delta.
                self.items.push(ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![crate::models::ContentItem::OutputText {
                        text: delta.to_string(),
                    }],
                });
            }
        }
    }
}

/// Anything that is not a system message or "reasoning" message is considered
/// an API message.
fn is_api_message(message: &ResponseItem) -> bool {
    match message {
        ResponseItem::Message { role, .. } => role.as_str() != "system",
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::Reasoning { .. } => true,
        ResponseItem::Other => false,
    }
}

/// Helper to append the textual content from `src` into `dst` in place.
fn append_text_content(
    dst: &mut Vec<crate::models::ContentItem>,
    src: &Vec<crate::models::ContentItem>,
) {
    for c in src {
        if let crate::models::ContentItem::OutputText { text } = c {
            append_text_delta(dst, text);
        }
    }
}

/// Append a single text delta to the last OutputText item in `content`, or
/// push a new OutputText item if none exists.
fn append_text_delta(content: &mut Vec<crate::models::ContentItem>, delta: &str) {
    if let Some(crate::models::ContentItem::OutputText { text }) = content
        .iter_mut()
        .rev()
        .find(|c| matches!(c, crate::models::ContentItem::OutputText { .. }))
    {
        text.push_str(delta);
    } else {
        content.push(crate::models::ContentItem::OutputText {
            text: delta.to_string(),
        });
    }
}
