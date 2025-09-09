use codex_core::protocol::ConversationHistoryResponseEvent;
use codex_core::protocol::Event;
use codex_file_search::FileMatch;

use crate::history_cell::HistoryCell;

use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_core::protocol_config_types::ReasoningEffort;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Start a new session.
    NewSession,

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(codex_core::protocol::Op),

    /// Kick off an asynchronous file search for the given query (text after
    /// the `@`). Previous searches may be cancelled by the app layer so there
    /// is at most one in-flight search.
    StartFileSearch(String),

    /// Result of a completed asynchronous file search. The `query` echoes the
    /// original search term so the UI can decide whether the results are
    /// still relevant.
    FileSearchResult {
        query: String,
        matches: Vec<FileMatch>,
    },

    /// Result of computing a `/diff` command.
    DiffResult(String),

    InsertHistoryCell(Box<dyn HistoryCell>),

    StartCommitAnimation,
    StopCommitAnimation,
    CommitTick,

    /// Update the current reasoning effort in the running app and widget.
    UpdateReasoningEffort(ReasoningEffort),

    /// Update the current model slug in the running app and widget.
    UpdateModel(String),

    /// Update the current approval policy in the running app and widget.
    UpdateAskForApprovalPolicy(AskForApproval),

    /// Update the current sandbox policy in the running app and widget.
    UpdateSandboxPolicy(SandboxPolicy),

    /// Forwarded conversation history snapshot from the current conversation.
    ConversationHistory(ConversationHistoryResponseEvent),

    /// Result of a locally executed shell command triggered by a leading '!'.
    /// Identified by `call_id` that was assigned when starting the command.
    LocalExecResult {
        call_id: String,
        /// Full argv used for display (e.g., ["bash","-lc", "echo hi"]).
        command: Vec<String>,
        /// Parsed metadata used to render exploring-style summaries when applicable.
        parsed_cmd: Vec<codex_protocol::parse_command::ParsedCommand>,
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration: std::time::Duration,
    },
}
