use codex_otel::otel_event_manager::OtelEventManager;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ShellToolCallParams;
use codex_protocol::protocol::FileChange;
use mcp_types::CallToolResult;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::codex::Session;
use crate::codex::TurnContext;
use crate::turn_diff_tracker::TurnDiffTracker;

pub struct ToolInvocation<'a> {
    pub session: &'a Session,
    pub turn: &'a TurnContext,
    pub tracker: &'a mut TurnDiffTracker,
    pub sub_id: &'a str,
    pub call_id: String,
    pub tool_name: String,
    pub payload: ToolPayload,
}

#[derive(Clone)]
pub enum ToolPayload {
    Function {
        arguments: String,
    },
    Custom {
        input: String,
    },
    LocalShell {
        params: ShellToolCallParams,
    },
    UnifiedExec {
        arguments: String,
    },
    Mcp {
        server: String,
        tool: String,
        arguments: Option<serde_json::Value>,
        raw_arguments: String,
    },
}

impl ToolPayload {
    pub fn log_payload(&self) -> Cow<'_, str> {
        match self {
            ToolPayload::Function { arguments } => Cow::Borrowed(arguments),
            ToolPayload::Custom { input } => Cow::Borrowed(input),
            ToolPayload::LocalShell { params } => Cow::Owned(params.command.join(" ")),
            ToolPayload::UnifiedExec { arguments } => Cow::Borrowed(arguments),
            ToolPayload::Mcp { raw_arguments, .. } => Cow::Borrowed(raw_arguments),
        }
    }
}

#[derive(Clone)]
pub enum ToolOutput {
    Function {
        content: String,
        success: bool,
    },
    Mcp {
        result: Result<CallToolResult, String>,
    },
}

impl ToolOutput {
    pub fn log_preview(&self) -> String {
        match self {
            ToolOutput::Function { content, .. } => content.clone(),
            ToolOutput::Mcp { result } => format!("{result:?}"),
        }
    }

    pub fn into_response(self, call_id: &str) -> ResponseInputItem {
        match self {
            ToolOutput::Function { content, success } => ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    content,
                    success: Some(success),
                },
            },
            ToolOutput::Mcp { result } => ResponseInputItem::McpToolCallOutput {
                call_id: call_id.to_string(),
                result,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecCommandContext {
    pub(crate) sub_id: String,
    pub(crate) call_id: String,
    pub(crate) command_for_display: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) apply_patch: Option<crate::codex::ApplyPatchCommandContext>,
    pub(crate) tool_name: String,
    pub(crate) otel_event_manager: OtelEventManager,
}

#[derive(Clone, Debug)]
pub(crate) struct ApplyPatchCommandContext {
    pub(crate) user_explicitly_approved_this_action: bool,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
}
