use crate::models::ResponseInputItem;

/// Transcript that needs to be maintained for ZDR clients for which
/// previous_response_id is not available, so we must include the transcript
/// with every API call.
#[derive(Debug, Clone)]
pub(crate) struct ZdrTranscript {
    items: Vec<ResponseInputItem>,
}

impl ZdrTranscript {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub(crate) fn add_item(&mut self, item: ResponseInputItem) {
        if is_api_message(&item) {
            // Note agent-loop.ts also does filtering on some of the fields.
            self.items.push(item);
        }
    }
}

fn is_api_message(message: &ResponseInputItem) -> bool {
    !matches!(message, ResponseInputItem::Message { role, .. } if role.as_str() == "system")
}
