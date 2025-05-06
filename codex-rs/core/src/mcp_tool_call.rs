use tracing::error;

use crate::codex::Session;
use crate::models::FunctionCallOutputPayload;
use crate::models::ResponseInputItem;
use crate::protocol::Event;
use crate::protocol::EventMsg;

/// Handles the specified tool call dispatches the appropriate
/// `McpToolCallBegin` and `McpToolCallEnd` events to the `Session`.
pub(crate) async fn handle_mcp_tool_call(
    sess: &Session,
    sub_id: &str,
    call_id: String,
    server: String,
    tool_name: String,
    arguments: String,
) -> ResponseInputItem {
    // Attempt to route to external MCP server.
    let arguments_value: Option<serde_json::Value> = serde_json::from_str(&arguments).ok();

    let tool_call_begin_event = EventMsg::McpToolCallBegin {
        call_id: call_id.clone(),
        server: server.clone(),
        tool: tool_name.clone(),
        arguments: arguments_value.clone(),
    };
    if let Err(e) = sess
        .tx_event
        .send(Event {
            id: sub_id.to_string(),
            msg: tool_call_begin_event,
        })
        .await
    {
        error!("failed to send tool call begin event: {e}");
    }

    let (tool_call_end_event, tool_call_err) = match sess
        .mcp
        .call_tool(&server, &tool_name, arguments_value)
        .await
    {
        Ok(result) => (
            EventMsg::McpToolCallEnd {
                call_id,
                success: !result.is_error.unwrap_or(false),
                result: Some(result),
            },
            None,
        ),
        Err(e) => (
            EventMsg::McpToolCallEnd {
                call_id,
                success: false,
                result: None,
            },
            Some(e),
        ),
    };
    if let Err(e) = sess
        .tx_event
        .send(Event {
            id: sub_id.to_string(),
            msg: tool_call_end_event.clone(),
        })
        .await
    {
        error!("failed to send tool call end event: {e}");
    }

    let EventMsg::McpToolCallEnd {
        call_id,
        success,
        result,
    } = tool_call_end_event
    else {
        unimplemented!("unexpected event type");
    };

    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputPayload {
            content: result.map_or_else(
                || format!("err: {tool_call_err:?}"),
                |result| {
                    serde_json::to_string(&result)
                        .unwrap_or_else(|e| format!("JSON serialization error: {e}"))
                },
            ),
            success: Some(success),
        },
    }
}
