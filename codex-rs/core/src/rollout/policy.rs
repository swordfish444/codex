use crate::protocol::EventMsg;
use codex_protocol::models::ResponseItem;

/// Whether a `ResponseItem` should be persisted in rollout files.
#[inline]
pub(crate) fn is_persisted_response_item(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. } => true,
        ResponseItem::WebSearchCall { .. } | ResponseItem::Other => false,
    }
}

/// Whether an `EventMsg` should be persisted in rollout files.
///
/// Keep only high-signal, compact items. Avoid deltas and verbose streams.
#[inline]
pub(crate) fn is_persisted_event_msg(event: &EventMsg) -> bool {
    match event {
        // Core content to replay UI meaningfully
        EventMsg::AgentMessage(_)
        | EventMsg::AgentReasoning(_)
        | EventMsg::TokenCount(_)
        | EventMsg::UserMessage(_) => true,

        // Everything else is either transient, redundant, or too verbose
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentMessageDeltaEvent;
    use crate::protocol::AgentMessageEvent;
    use crate::protocol::AgentReasoningDeltaEvent;
    use crate::protocol::AgentReasoningEvent;
    use crate::protocol::ApplyPatchApprovalRequestEvent;
    use crate::protocol::ExecCommandEndEvent;
    use crate::protocol::TokenCountEvent;

    #[test]
    fn test_event_persistence_policy() {
        assert!(is_persisted_event_msg(&EventMsg::AgentMessage(
            AgentMessageEvent {
                message: "hi".to_string(),
            }
        )));
        assert!(is_persisted_event_msg(&EventMsg::AgentReasoning(
            AgentReasoningEvent {
                text: "think".to_string(),
            }
        )));
        assert!(is_persisted_event_msg(&EventMsg::TokenCount(
            TokenCountEvent { info: None }
        )));

        assert!(!is_persisted_event_msg(&EventMsg::AgentMessageDelta(
            AgentMessageDeltaEvent {
                delta: "d".to_string(),
            }
        )));
        assert!(!is_persisted_event_msg(&EventMsg::AgentReasoningDelta(
            AgentReasoningDeltaEvent {
                delta: "d".to_string(),
            }
        )));
        assert!(!is_persisted_event_msg(&EventMsg::ExecCommandEnd(
            ExecCommandEndEvent {
                call_id: "c".to_string(),
                stdout: Default::default(),
                stderr: Default::default(),
                aggregated_output: Default::default(),
                exit_code: 0,
                duration: std::time::Duration::from_secs(0),
                formatted_output: String::new(),
            }
        )));
        use crate::protocol::FileChange;
        use std::collections::HashMap;
        use std::path::PathBuf;
        assert!(!is_persisted_event_msg(
            &EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                call_id: "c".to_string(),
                changes: HashMap::<PathBuf, FileChange>::new(),
                reason: None,
                grant_root: None,
            })
        ));
    }
}
