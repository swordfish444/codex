use serde::Serialize;

/// Cross-host notification payloads emitted by the agent runtime.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UserNotification {
    #[serde(rename_all = "kebab-case")]
    AgentTurnComplete {
        turn_id: String,
        /// Messages submitted by the user to start the turn.
        input_messages: Vec<String>,
        /// Final assistant message emitted at turn completion.
        last_assistant_message: Option<String>,
    },
}
