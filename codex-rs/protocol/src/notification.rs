use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Display)]
#[serde(tag = "method", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum NotificationMessage {
    Conversation(ConversationNotification),
    ShutdownComplete,
}

/// Notification associated with a conversation. The `conversation_id` is key
/// so clients can dispatch messages to the correct conversation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConversationNotification {
    conversation_id: Uuid,
    #[serde(flatten)]
    message: ConversationNotificationMessage,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Display)]
#[serde(tag = "type", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
enum ConversationNotificationMessage {
    Initialized(ConversationInitialized),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct ConversationInitialized {
    model: String,
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde::de::Error as _;
    use serde_json::json;

    // The idea is that the way we map `NotificationMessage` to an MCP
    // notification is to use the tuple type as the `params`.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct JsonrpcNotification {
        jsonrpc: String,
        method: String,
        params: serde_json::Value,
    }

    // Example of how JSON-RPC serialization could work.
    fn to_jsonrpc_message(
        notification: NotificationMessage,
    ) -> Result<serde_json::Value, serde_json::Error> {
        let method = notification.to_string();
        let params = match notification {
            NotificationMessage::Conversation(notification) => serde_json::to_value(notification)?,
            NotificationMessage::ShutdownComplete => serde_json::Value::Null,
        };

        let jsonrpc_notification = JsonrpcNotification {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        };

        serde_json::to_value(jsonrpc_notification)
    }

    fn from_jsonrpc_message(
        jsonrpc_notification: JsonrpcNotification,
    ) -> Result<NotificationMessage, serde_json::Error> {
        let JsonrpcNotification {
            jsonrpc: _,
            method,
            params,
        } = jsonrpc_notification;

        match method.as_str() {
            "conversation" => {
                let conversation_notification: ConversationNotification =
                    serde_json::from_value(params)?;
                Ok(NotificationMessage::Conversation(conversation_notification))
            }
            "shutdown_complete" => Ok(NotificationMessage::ShutdownComplete),
            _ => Err(serde_json::Error::custom(format!(
                "Unknown method: {method}"
            ))),
        }
    }

    #[test]
    fn test_serialize_notification_message_conversation() {
        let conversation_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let message = NotificationMessage::Conversation(ConversationNotification {
            conversation_id,
            message: ConversationNotificationMessage::Initialized(ConversationInitialized {
                model: "gpt-5".to_string(),
            }),
        });
        assert_eq!(
            json!({
                "method": "conversation",
                "conversation_id": conversation_id.to_string(),
                "type": "initialized",
                "model": "gpt-5",
            }),
            serde_json::to_value(message.clone()).unwrap()
        );

        let expected_jsonrpc_message = json!({
            "jsonrpc": "2.0",
            "method": "conversation",
            "params": {
                "conversation_id": conversation_id.to_string(),
                "type": "initialized",
                "model": "gpt-5",
            }
        });
        assert_eq!(
            expected_jsonrpc_message,
            to_jsonrpc_message(message.clone()).unwrap()
        );

        let serialized_json_rpc_message = serde_json::to_string(&expected_jsonrpc_message).unwrap();
        let deserialized_json_rpc_message =
            serde_json::from_str::<JsonrpcNotification>(&serialized_json_rpc_message).unwrap();
        assert_eq!(
            JsonrpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "conversation".to_string(),
                params: json!({
                    "conversation_id": conversation_id.to_string(),
                    "type": "initialized",
                    "model": "gpt-5",
                }),
            },
            deserialized_json_rpc_message
        );

        assert_eq!(
            message,
            from_jsonrpc_message(deserialized_json_rpc_message).unwrap()
        );
    }
}
