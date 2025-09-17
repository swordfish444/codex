//! Defines the protocol for a Codex session between a client and an agent.
//!
//! Uses a SQ (Submission Queue) / EQ (Event Queue) pattern to asynchronously communicate
//! between user and agent.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::custom_prompts::CustomPrompt;
use crate::mcp_protocol::ConversationId;
use crate::message_history::HistoryEntry;
use crate::models::ContentItem;
use crate::models::ResponseItem;
use crate::num_format::format_with_separators;
use crate::parse_command::ParsedCommand;
use crate::plan_tool::UpdatePlanArgs;
use mcp_types::CallToolResult;
use mcp_types::Tool as McpTool;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use strum_macros::Display;
use ts_rs::TS;

/// Open/close tags for special user-input blocks. Used across crates to avoid
/// duplicated hardcoded strings.
pub const USER_INSTRUCTIONS_OPEN_TAG: &str = "<user_instructions>";
pub const USER_INSTRUCTIONS_CLOSE_TAG: &str = "</user_instructions>";
pub const ENVIRONMENT_CONTEXT_OPEN_TAG: &str = "<environment_context>";
pub const ENVIRONMENT_CONTEXT_CLOSE_TAG: &str = "</environment_context>";
pub const USER_MESSAGE_BEGIN: &str = "## My request for Codex:";

/// Submission Queue Entry - requests from user
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Submission {
    /// Unique id for this Submission to correlate with Events
    pub id: String,
    /// Payload
    pub op: Op,
}

/// Submission operation
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum Op {
    /// Abort current task.
    /// This server sends [`EventMsg::TurnAborted`] in response.
    Interrupt,

    /// Input from the user
    UserInput {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,
    },

    /// Similar to [`Op::UserInput`], but contains additional context required
    /// for a turn of a [`crate::codex_conversation::CodexConversation`].
    UserTurn {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,

        /// `cwd` to use with the [`SandboxPolicy`] and potentially tool calls
        /// such as `local_shell`.
        cwd: PathBuf,

        /// Policy to use for command approval.
        approval_policy: AskForApproval,

        /// Policy to use for tool calls such as `local_shell`.
        sandbox_policy: SandboxPolicy,

        /// Must be a valid model slug for the [`crate::client::ModelClient`]
        /// associated with this conversation.
        model: String,

        /// Will only be honored if the model is configured to use reasoning.
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<ReasoningEffortConfig>,

        /// Will only be honored if the model is configured to use reasoning.
        summary: ReasoningSummaryConfig,
    },

    /// Override parts of the persistent turn context for subsequent turns.
    ///
    /// All fields are optional; when omitted, the existing value is preserved.
    /// This does not enqueue any input – it only updates defaults used for
    /// future `UserInput` turns.
    OverrideTurnContext {
        /// Updated `cwd` for sandbox/tool calls.
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,

        /// Updated command approval policy.
        #[serde(skip_serializing_if = "Option::is_none")]
        approval_policy: Option<AskForApproval>,

        /// Updated sandbox policy for tool calls.
        #[serde(skip_serializing_if = "Option::is_none")]
        sandbox_policy: Option<SandboxPolicy>,

        /// Updated model slug. When set, the model family is derived
        /// automatically.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,

        /// Updated reasoning effort (honored only for reasoning-capable models).
        ///
        /// Use `Some(Some(_))` to set a specific effort, `Some(None)` to clear
        /// the effort, or `None` to leave the existing value unchanged.
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<Option<ReasoningEffortConfig>>,

        /// Updated reasoning summary preference (honored only for reasoning-capable models).
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummaryConfig>,
    },

    /// Approve a command execution
    ExecApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Approve a code patch
    PatchApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Append an entry to the persistent cross-session message history.
    ///
    /// Note the entry is not guaranteed to be logged if the user has
    /// history disabled, it matches the list of "sensitive" patterns, etc.
    AddToHistory {
        /// The message text to be stored.
        text: String,
    },

    /// Request a single history entry identified by `log_id` + `offset`.
    GetHistoryEntryRequest { offset: usize, log_id: u64 },

    /// Request the full in-memory conversation transcript for the current session.
    /// Reply is delivered via `EventMsg::ConversationHistory`.
    GetPath,

    /// Request the list of MCP tools available across all configured servers.
    /// Reply is delivered via `EventMsg::McpListToolsResponse`.
    ListMcpTools,

    /// Request the list of available custom prompts.
    ListCustomPrompts,

    /// Request the agent to summarize the current conversation context.
    /// The agent will use its existing context (either conversation history or previous response id)
    /// to generate a summary which will be returned as an AgentMessage event.
    Compact,

    /// Request a code review from the agent.
    Review { review_request: ReviewRequest },

    /// Request to shut down codex instance.
    Shutdown,
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, TS)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AskForApproval {
    /// Under this policy, only "known safe" commands—as determined by
    /// `is_safe_command()`—that **only read files** are auto‑approved.
    /// Everything else will ask the user to approve.
    #[serde(rename = "untrusted")]
    #[strum(serialize = "untrusted")]
    UnlessTrusted,

    /// *All* commands are auto‑approved, but they are expected to run inside a
    /// sandbox where network access is disabled and writes are confined to a
    /// specific set of paths. If the command fails, it will be escalated to
    /// the user to approve execution without a sandbox.
    OnFailure,

    /// The model decides when to ask the user for approval.
    #[default]
    OnRequest,

    /// Never ask the user to approve commands. Failures are immediately returned
    /// to the model, and never escalated to the user for approval.
    Never,
}

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, TS)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever. Use with caution.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access to the entire file-system.
    #[serde(rename = "read-only")]
    ReadOnly,

    /// Same as `ReadOnly` but additionally grants write access to the current
    /// working directory ("workspace").
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        /// Additional folders (beyond cwd and possibly TMPDIR) that should be
        /// writable from within the sandbox.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,

        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default)]
        network_access: bool,

        /// When set to `true`, will NOT include the per-user `TMPDIR`
        /// environment variable among the default writable roots. Defaults to
        /// `false`.
        #[serde(default)]
        exclude_tmpdir_env_var: bool,

        /// When set to `true`, will NOT include the `/tmp` among the default
        /// writable roots on UNIX. Defaults to `false`.
        #[serde(default)]
        exclude_slash_tmp: bool,
    },
}

/// A writable root path accompanied by a list of subpaths that should remain
/// read‑only even when the root is writable. This is primarily used to ensure
/// top‑level VCS metadata directories (e.g. `.git`) under a writable root are
/// not modified by the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot {
    /// Absolute path, by construction.
    pub root: PathBuf,

    /// Also absolute paths, by construction.
    pub read_only_subpaths: Vec<PathBuf>,
}

impl WritableRoot {
    pub fn is_path_writable(&self, path: &Path) -> bool {
        // Check if the path is under the root.
        if !path.starts_with(&self.root) {
            return false;
        }

        // Check if the path is under any of the read-only subpaths.
        for subpath in &self.read_only_subpaths {
            if path.starts_with(subpath) {
                return false;
            }
        }

        true
    }
}

impl FromStr for SandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl SandboxPolicy {
    /// Returns a policy with read-only disk access and no network.
    pub fn new_read_only_policy() -> Self {
        SandboxPolicy::ReadOnly
    }

    /// Returns a policy that can read the entire disk, but can only write to
    /// the current working directory and the per-user tmp dir on macOS. It does
    /// not allow network access.
    pub fn new_workspace_write_policy() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    /// Always returns `true`; restricting read access is not supported.
    pub fn has_full_disk_read_access(&self) -> bool {
        true
    }

    pub fn has_full_disk_write_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { .. } => false,
        }
    }

    pub fn has_full_network_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }

    /// Returns the list of writable roots (tailored to the current working
    /// directory) together with subpaths that should remain read‑only under
    /// each writable root.
    pub fn get_writable_roots_with_cwd(&self, cwd: &Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::DangerFullAccess => Vec::new(),
            SandboxPolicy::ReadOnly => Vec::new(),
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
                network_access: _,
            } => {
                // Start from explicitly configured writable roots.
                let mut roots: Vec<PathBuf> = writable_roots.clone();

                // Always include defaults: cwd, /tmp (if present on Unix), and
                // on macOS, the per-user TMPDIR unless explicitly excluded.
                roots.push(cwd.to_path_buf());

                // Include /tmp on Unix unless explicitly excluded.
                if cfg!(unix) && !exclude_slash_tmp {
                    let slash_tmp = PathBuf::from("/tmp");
                    if slash_tmp.is_dir() {
                        roots.push(slash_tmp);
                    }
                }

                // Include $TMPDIR unless explicitly excluded. On macOS, TMPDIR
                // is per-user, so writes to TMPDIR should not be readable by
                // other users on the system.
                //
                // By comparison, TMPDIR is not guaranteed to be defined on
                // Linux or Windows, but supporting it here gives users a way to
                // provide the model with their own temporary directory without
                // having to hardcode it in the config.
                if !exclude_tmpdir_env_var
                    && let Some(tmpdir) = std::env::var_os("TMPDIR")
                    && !tmpdir.is_empty()
                {
                    roots.push(PathBuf::from(tmpdir));
                }

                // For each root, compute subpaths that should remain read-only.
                roots
                    .into_iter()
                    .map(|writable_root| {
                        let mut subpaths = Vec::new();
                        let top_level_git = writable_root.join(".git");
                        if top_level_git.is_dir() {
                            subpaths.push(top_level_git);
                        }
                        WritableRoot {
                            root: writable_root,
                            read_only_subpaths: subpaths,
                        }
                    })
                    .collect()
            }
        }
    }
}

/// User input
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    Text {
        text: String,
    },
    /// Pre‑encoded data: URI image.
    Image {
        image_url: String,
    },

    /// Local image path provided by the user.  This will be converted to an
    /// `Image` variant (base64 data URL) during request serialization.
    LocalImage {
        path: std::path::PathBuf,
    },
}

/// Event Queue Entry - events from agent
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Event {
    /// Submission `id` that this event is correlated with.
    pub id: String,
    /// Payload
    pub msg: EventMsg,
}

/// Response event from the agent
/// NOTE: Make sure none of these values have optional types, as it will mess up the extension code-gen.
#[derive(Debug, Clone, Deserialize, Serialize, Display, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EventMsg {
    /// Error while executing a submission
    Error(ErrorEvent),

    /// Agent has started a task
    TaskStarted(TaskStartedEvent),

    /// Agent has completed all actions
    TaskComplete(TaskCompleteEvent),

    /// Usage update for the current session, including totals and last turn.
    /// Optional means unknown — UIs should not display when `None`.
    TokenCount(TokenCountEvent),

    /// Agent text output message
    AgentMessage(AgentMessageEvent),

    /// User/system input message (what was sent to the model)
    UserMessage(UserMessageEvent),

    /// Agent text output delta message
    AgentMessageDelta(AgentMessageDeltaEvent),

    /// Reasoning event from agent.
    AgentReasoning(AgentReasoningEvent),

    /// Agent reasoning delta event from agent.
    AgentReasoningDelta(AgentReasoningDeltaEvent),

    /// Raw chain-of-thought from agent.
    AgentReasoningRawContent(AgentReasoningRawContentEvent),

    /// Agent reasoning content delta event from agent.
    AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent),
    /// Signaled when the model begins a new reasoning summary section (e.g., a new titled block).
    AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),

    /// Ack the client's configure message.
    SessionConfigured(SessionConfiguredEvent),

    McpToolCallBegin(McpToolCallBeginEvent),

    McpToolCallEnd(McpToolCallEndEvent),

    WebSearchBegin(WebSearchBeginEvent),

    WebSearchEnd(WebSearchEndEvent),

    /// Notification that the server is about to execute a command.
    ExecCommandBegin(ExecCommandBeginEvent),

    /// Incremental chunk of output from a running command.
    ExecCommandOutputDelta(ExecCommandOutputDeltaEvent),

    ExecCommandEnd(ExecCommandEndEvent),

    ExecApprovalRequest(ExecApprovalRequestEvent),

    ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),

    BackgroundEvent(BackgroundEventEvent),

    /// Notification that a model stream experienced an error or disconnect
    /// and the system is handling it (e.g., retrying with backoff).
    StreamError(StreamErrorEvent),

    /// Notification that the agent is about to apply a code patch. Mirrors
    /// `ExecCommandBegin` so front‑ends can show progress indicators.
    PatchApplyBegin(PatchApplyBeginEvent),

    /// Notification that a patch application has finished.
    PatchApplyEnd(PatchApplyEndEvent),

    TurnDiff(TurnDiffEvent),

    /// Response to GetHistoryEntryRequest.
    GetHistoryEntryResponse(GetHistoryEntryResponseEvent),

    /// List of MCP tools available to the agent.
    McpListToolsResponse(McpListToolsResponseEvent),

    /// List of custom prompts available to the agent.
    ListCustomPromptsResponse(ListCustomPromptsResponseEvent),

    PlanUpdate(UpdatePlanArgs),

    TurnAborted(TurnAbortedEvent),

    /// Notification that the agent is shutting down.
    ShutdownComplete,

    ConversationPath(ConversationPathResponseEvent),

    /// Entered review mode.
    EnteredReviewMode(ReviewRequest),

    /// Exited review mode with an optional final result to apply.
    ExitedReviewMode(ExitedReviewModeEvent),
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ExitedReviewModeEvent {
    pub review_output: Option<ReviewOutputEvent>,
}

// Individual event payload types matching each `EventMsg` variant.

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TaskCompleteEvent {
    pub last_agent_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TaskStartedEvent {
    pub model_context_window: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, TS)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    pub model_context_window: Option<u64>,
}

impl TokenUsageInfo {
    pub fn new_or_append(
        info: &Option<TokenUsageInfo>,
        last: &Option<TokenUsage>,
        model_context_window: Option<u64>,
    ) -> Option<Self> {
        if info.is_none() && last.is_none() {
            return None;
        }

        let mut info = match info {
            Some(info) => info.clone(),
            None => Self {
                total_token_usage: TokenUsage::default(),
                last_token_usage: TokenUsage::default(),
                model_context_window,
            },
        };
        if let Some(last) = last {
            info.append_last_usage(last);
        }
        Some(info)
    }

    pub fn append_last_usage(&mut self, last: &TokenUsage) {
        self.total_token_usage.add_assign(last);
        self.last_token_usage = last.clone();
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TokenCountEvent {
    pub info: Option<TokenUsageInfo>,
}

// Includes prompts, tools and space to call compact.
const BASELINE_TOKENS: u64 = 12000;

impl TokenUsage {
    pub fn is_zero(&self) -> bool {
        self.total_tokens == 0
    }

    pub fn cached_input(&self) -> u64 {
        self.cached_input_tokens
    }

    pub fn non_cached_input(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cached_input())
    }

    /// Primary count for display as a single absolute value: non-cached input + output.
    pub fn blended_total(&self) -> u64 {
        self.non_cached_input() + self.output_tokens
    }

    /// For estimating what % of the model's context window is used, we need to account
    /// for reasoning output tokens from prior turns being dropped from the context window.
    /// We approximate this here by subtracting reasoning output tokens from the total.
    /// This will be off for the current turn and pending function calls.
    pub fn tokens_in_context_window(&self) -> u64 {
        self.total_tokens
            .saturating_sub(self.reasoning_output_tokens)
    }

    /// Estimate the remaining user-controllable percentage of the model's context window.
    ///
    /// `context_window` is the total size of the model's context window.
    /// `BASELINE_TOKENS` should capture tokens that are always present in
    /// the context (e.g., system prompt and fixed tool instructions) so that
    /// the percentage reflects the portion the user can influence.
    ///
    /// This normalizes both the numerator and denominator by subtracting the
    /// baseline, so immediately after the first prompt the UI shows 100% left
    /// and trends toward 0% as the user fills the effective window.
    pub fn percent_of_context_window_remaining(&self, context_window: u64) -> u8 {
        if context_window <= BASELINE_TOKENS {
            return 0;
        }

        let effective_window = context_window - BASELINE_TOKENS;
        let used = self
            .tokens_in_context_window()
            .saturating_sub(BASELINE_TOKENS);
        let remaining = effective_window.saturating_sub(used);
        ((remaining as f32 / effective_window as f32) * 100.0).clamp(0.0, 100.0) as u8
    }

    /// In-place element-wise sum of token counts.
    pub fn add_assign(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FinalOutput {
    pub token_usage: TokenUsage,
}

impl From<TokenUsage> for FinalOutput {
    fn from(token_usage: TokenUsage) -> Self {
        Self { token_usage }
    }
}

impl fmt::Display for FinalOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token_usage = &self.token_usage;

        write!(
            f,
            "Token usage: total={} input={}{} output={}{}",
            format_with_separators(token_usage.blended_total()),
            format_with_separators(token_usage.non_cached_input()),
            if token_usage.cached_input() > 0 {
                format!(
                    " (+ {} cached)",
                    format_with_separators(token_usage.cached_input())
                )
            } else {
                String::new()
            },
            format_with_separators(token_usage.output_tokens),
            if token_usage.reasoning_output_tokens > 0 {
                format!(
                    " (reasoning {})",
                    format_with_separators(token_usage.reasoning_output_tokens)
                )
            } else {
                String::new()
            }
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InputMessageKind {
    /// Plain user text (default)
    Plain,
    /// XML-wrapped user instructions (<user_instructions>...)
    UserInstructions,
    /// XML-wrapped environment context (<environment_context>...)
    EnvironmentContext,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct UserMessageEvent {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<InputMessageKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

impl<T, U> From<(T, U)> for InputMessageKind
where
    T: AsRef<str>,
    U: AsRef<str>,
{
    fn from(value: (T, U)) -> Self {
        let (_role, message) = value;
        let message = message.as_ref();
        let trimmed = message.trim();
        if starts_with_ignore_ascii_case(trimmed, ENVIRONMENT_CONTEXT_OPEN_TAG)
            && ends_with_ignore_ascii_case(trimmed, ENVIRONMENT_CONTEXT_CLOSE_TAG)
        {
            InputMessageKind::EnvironmentContext
        } else if starts_with_ignore_ascii_case(trimmed, USER_INSTRUCTIONS_OPEN_TAG)
            && ends_with_ignore_ascii_case(trimmed, USER_INSTRUCTIONS_CLOSE_TAG)
        {
            InputMessageKind::UserInstructions
        } else {
            InputMessageKind::Plain
        }
    }
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    let text_bytes = text.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    text_bytes.len() >= prefix_bytes.len()
        && text_bytes
            .iter()
            .zip(prefix_bytes.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn ends_with_ignore_ascii_case(text: &str, suffix: &str) -> bool {
    let text_bytes = text.as_bytes();
    let suffix_bytes = suffix.as_bytes();
    text_bytes.len() >= suffix_bytes.len()
        && text_bytes[text_bytes.len() - suffix_bytes.len()..]
            .iter()
            .zip(suffix_bytes.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentReasoningEvent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentReasoningRawContentEvent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentReasoningRawContentDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentReasoningSectionBreakEvent {}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct AgentReasoningDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct McpInvocation {
    /// Name of the MCP server as defined in the config.
    pub server: String,
    /// Name of the tool as given by the MCP server.
    pub tool: String,
    /// Arguments to the tool call.
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct McpToolCallBeginEvent {
    /// Identifier so this can be paired with the McpToolCallEnd event.
    pub call_id: String,
    pub invocation: McpInvocation,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct McpToolCallEndEvent {
    /// Identifier for the corresponding McpToolCallBegin that finished.
    pub call_id: String,
    pub invocation: McpInvocation,
    #[ts(type = "string")]
    pub duration: Duration,
    /// Result of the tool call. Note this could be an error.
    pub result: Result<CallToolResult, String>,
}

impl McpToolCallEndEvent {
    pub fn is_success(&self) -> bool {
        match &self.result {
            Ok(result) => !result.is_error.unwrap_or(false),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct WebSearchBeginEvent {
    pub call_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct WebSearchEndEvent {
    pub call_id: String,
    pub query: String,
}

/// Response payload for `Op::GetHistory` containing the current session's
/// in-memory transcript.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ConversationPathResponseEvent {
    pub conversation_id: ConversationId,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ResumedHistory {
    pub conversation_id: ConversationId,
    pub history: Vec<RolloutItem>,
    pub rollout_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub enum InitialHistory {
    New,
    Resumed(ResumedHistory),
    Forked(Vec<RolloutItem>),
}

impl InitialHistory {
    pub fn get_rollout_items(&self) -> Vec<RolloutItem> {
        match self {
            InitialHistory::New => Vec::new(),
            InitialHistory::Resumed(resumed) => resumed.history.clone(),
            InitialHistory::Forked(items) => items.clone(),
        }
    }

    pub fn get_event_msgs(&self) -> Option<Vec<EventMsg>> {
        match self {
            InitialHistory::New => None,
            InitialHistory::Resumed(resumed) => Some(
                resumed
                    .history
                    .iter()
                    .filter_map(|ri| match ri {
                        RolloutItem::EventMsg(ev) => Some(ev.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            InitialHistory::Forked(items) => Some(
                items
                    .iter()
                    .filter_map(|ri| match ri {
                        RolloutItem::EventMsg(ev) => Some(ev.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, TS)]
pub struct SessionMeta {
    pub id: ConversationId,
    pub timestamp: String,
    pub cwd: PathBuf,
    pub originator: String,
    pub cli_version: String,
    pub instructions: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, TS)]
pub struct SessionMetaLine {
    #[serde(flatten)]
    pub meta: SessionMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RolloutItem {
    SessionMeta(SessionMetaLine),
    ResponseItem(ResponseItem),
    Compacted(CompactedItem),
    TurnContext(TurnContextItem),
    EventMsg(EventMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
pub struct CompactedItem {
    pub message: String,
}

impl From<CompactedItem> for ResponseItem {
    fn from(value: CompactedItem) -> Self {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: value.message,
            }],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
pub struct TurnContextItem {
    pub cwd: PathBuf,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffortConfig>,
    pub summary: ReasoningSummaryConfig,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RolloutLine {
    pub timestamp: String,
    #[serde(flatten)]
    pub item: RolloutItem,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
pub struct GitInfo {
    /// Current commit hash (SHA)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    /// Current branch name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Repository URL (if available from remote)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// Review request sent to the review session.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ReviewRequest {
    pub prompt: String,
    pub user_facing_hint: String,
}

/// Structured review result produced by a child review session.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ReviewOutputEvent {
    pub findings: Vec<ReviewFinding>,
    pub overall_correctness: String,
    pub overall_explanation: String,
    pub overall_confidence_score: f32,
}

impl Default for ReviewOutputEvent {
    fn default() -> Self {
        Self {
            findings: Vec::new(),
            overall_correctness: String::default(),
            overall_explanation: String::default(),
            overall_confidence_score: 0.0,
        }
    }
}

/// A single review finding describing an observed issue or recommendation.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ReviewFinding {
    pub title: String,
    pub body: String,
    pub confidence_score: f32,
    pub priority: i32,
    pub code_location: ReviewCodeLocation,
}

/// Location of the code related to a review finding.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ReviewCodeLocation {
    pub absolute_file_path: PathBuf,
    pub line_range: ReviewLineRange,
}

/// Inclusive line range in a file associated with the finding.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ReviewLineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ExecCommandBeginEvent {
    /// Identifier so this can be paired with the ExecCommandEnd event.
    pub call_id: String,
    /// The command to be executed.
    pub command: Vec<String>,
    /// The command's working directory if not the default cwd for the agent.
    pub cwd: PathBuf,
    pub parsed_cmd: Vec<ParsedCommand>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ExecCommandEndEvent {
    /// Identifier for the ExecCommandBegin that finished.
    pub call_id: String,
    /// Captured stdout
    pub stdout: String,
    /// Captured stderr
    pub stderr: String,
    /// Captured aggregated output
    #[serde(default)]
    pub aggregated_output: String,
    /// The command's exit code.
    pub exit_code: i32,
    /// The duration of the command execution.
    #[ts(type = "string")]
    pub duration: Duration,
    /// Formatted output from the command, as seen by the model.
    pub formatted_output: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputStream {
    Stdout,
    Stderr,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
pub struct ExecCommandOutputDeltaEvent {
    /// Identifier for the ExecCommandBegin that produced this chunk.
    pub call_id: String,
    /// Which stream produced this chunk.
    pub stream: ExecOutputStream,
    /// Raw bytes from the stream (may not be valid UTF-8).
    #[serde_as(as = "serde_with::base64::Base64")]
    #[ts(type = "string")]
    pub chunk: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ExecApprovalRequestEvent {
    /// Identifier for the associated exec call, if available.
    pub call_id: String,
    /// The command to be executed.
    pub command: Vec<String>,
    /// The command's working directory.
    pub cwd: PathBuf,
    /// Optional human-readable reason for the approval (e.g. retry without sandbox).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ApplyPatchApprovalRequestEvent {
    /// Responses API call id for the associated patch apply call, if available.
    pub call_id: String,
    pub changes: HashMap<PathBuf, FileChange>,
    /// Optional explanatory reason (e.g. request for extra write access).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When set, the agent is asking the user to allow writes under this root for the remainder of the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct BackgroundEventEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct StreamErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct PatchApplyBeginEvent {
    /// Identifier so this can be paired with the PatchApplyEnd event.
    pub call_id: String,
    /// If true, there was no ApplyPatchApprovalRequest for this patch.
    pub auto_approved: bool,
    /// The changes to be applied.
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct PatchApplyEndEvent {
    /// Identifier for the PatchApplyBegin that finished.
    pub call_id: String,
    /// Captured stdout (summary printed by apply_patch).
    pub stdout: String,
    /// Captured stderr (parser errors, IO failures, etc.).
    pub stderr: String,
    /// Whether the patch was applied successfully.
    pub success: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TurnDiffEvent {
    pub unified_diff: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct GetHistoryEntryResponseEvent {
    pub offset: usize,
    pub log_id: u64,
    /// The entry at the requested offset, if available and parseable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<HistoryEntry>,
}

/// Response payload for `Op::ListMcpTools`.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct McpListToolsResponseEvent {
    /// Fully qualified tool name -> tool definition.
    pub tools: std::collections::HashMap<String, McpTool>,
}

/// Response payload for `Op::ListCustomPrompts`.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct ListCustomPromptsResponseEvent {
    pub custom_prompts: Vec<CustomPrompt>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
pub struct SessionConfiguredEvent {
    /// Name left as session_id instead of conversation_id for backwards compatibility.
    pub session_id: ConversationId,

    /// Tell the client what model is being queried.
    pub model: String,

    /// The effort the model is putting into reasoning about the user's request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffortConfig>,

    /// Identifier of the history log file (inode on Unix, 0 otherwise).
    pub history_log_id: u64,

    /// Current number of entries in the history log.
    pub history_entry_count: usize,

    /// Optional initial messages (as events) for resumed sessions.
    /// When present, UIs can use these to seed the history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<EventMsg>>,

    pub rollout_path: PathBuf,
}

/// User's decision in response to an ExecApprovalRequest.
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    /// User has approved this command and the agent should execute it.
    Approved,

    /// User has approved this command and wants to automatically approve any
    /// future identical instances (`command` and `cwd` match exactly) for the
    /// remainder of the session.
    ApprovedForSession,

    /// User has denied this command and the agent should not execute it, but
    /// it should continue the session and try something else.
    #[default]
    Denied,

    /// User has denied this command and the agent should not do anything until
    /// the user's next command.
    Abort,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Add {
        content: String,
    },
    Delete {
        content: String,
    },
    Update {
        unified_diff: String,
        move_path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct Chunk {
    /// 1-based line index of the first line in the original file
    pub orig_index: u32,
    pub deleted_lines: Vec<String>,
    pub inserted_lines: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct TurnAbortedEvent {
    pub reason: TurnAbortReason,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum TurnAbortReason {
    Interrupted,
    Replaced,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LocalShellAction;
    use crate::models::LocalShellStatus;
    use crate::models::ReasoningItemContent;
    use crate::models::ReasoningItemReasoningSummary;
    use crate::models::WebSearchAction;
    use serde_json::Value;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    /// Serialize Event to verify that its JSON representation has the expected
    /// amount of nesting.
    #[test]
    fn serialize_event() {
        let conversation_id = ConversationId(uuid::uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8"));
        let rollout_file = NamedTempFile::new().unwrap();
        let event = Event {
            id: "1234".to_string(),
            msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id: conversation_id,
                model: "codex-mini-latest".to_string(),
                reasoning_effort: Some(ReasoningEffortConfig::default()),
                history_log_id: 0,
                history_entry_count: 0,
                initial_messages: None,
                rollout_path: rollout_file.path().to_path_buf(),
            }),
        };

        let expected = json!({
            "id": "1234",
            "msg": {
                "type": "session_configured",
                "session_id": "67e55044-10b1-426f-9247-bb680e5fe0c8",
                "model": "codex-mini-latest",
                "reasoning_effort": "medium",
                "history_log_id": 0,
                "history_entry_count": 0,
                "rollout_path": format!("{}", rollout_file.path().display()),
            }
        });
        assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }

    #[test]
    fn vec_u8_as_base64_serialization_and_deserialization() {
        let event = ExecCommandOutputDeltaEvent {
            call_id: "call21".to_string(),
            stream: ExecOutputStream::Stdout,
            chunk: vec![1, 2, 3, 4, 5],
        };
        let serialized = serde_json::to_string(&event).unwrap();
        assert_eq!(
            r#"{"call_id":"call21","stream":"stdout","chunk":"AQIDBAU="}"#,
            serialized,
        );

        let deserialized: ExecCommandOutputDeltaEvent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, event);
    }

    fn parse_rollout_line(value: Value, case: &str) -> RolloutLine {
        let serialized = serde_json::to_string(&value)
            .unwrap_or_else(|err| panic!("failed to serialize {case}: {err}"));
        serde_json::from_str(&serialized)
            .unwrap_or_else(|err| panic!("failed to parse {case}: {err}"))
    }

    #[test]
    fn deserialize_rollout_session_meta_lines() {
        let timestamp = "2025-01-02T03:04:05.678Z";
        let conversation_id = uuid::uuid!("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let cases: Vec<(&str, Value)> = vec![
            (
                "with_git",
                json!({
                    "timestamp": timestamp,
                    "type": "session_meta",
                    "payload": {
                        "id": conversation_id,
                        "timestamp": timestamp,
                        "cwd": "/workspace",
                        "originator": "codex-cli",
                        "cli_version": "1.0.0",
                        "instructions": "Remember the tests",
                        "git": {
                            "commit_hash": "abc123",
                            "branch": "main",
                            "repository_url": "https://example.com/repo.git"
                        }
                    }
                }),
            ),
            (
                "without_git",
                json!({
                    "timestamp": timestamp,
                    "type": "session_meta",
                    "payload": {
                        "id": conversation_id,
                        "timestamp": timestamp,
                        "cwd": "/workspace",
                        "originator": "codex-cli",
                        "cli_version": "1.0.0",
                        "instructions": null
                    }
                }),
            ),
        ];

        for (case, value) in cases {
            let parsed = parse_rollout_line(value, case);
            assert_eq!(parsed.timestamp, timestamp);
            match parsed.item {
                RolloutItem::SessionMeta(session_meta) => {
                    assert_eq!(session_meta.meta.id, ConversationId(conversation_id));
                    assert_eq!(session_meta.meta.cli_version, "1.0.0");
                    assert_eq!(session_meta.meta.originator, "codex-cli");
                    assert_eq!(session_meta.meta.cwd, PathBuf::from("/workspace"));
                    assert_eq!(session_meta.meta.timestamp, timestamp);
                    match case {
                        "with_git" => {
                            assert_eq!(
                                session_meta.meta.instructions.as_deref(),
                                Some("Remember the tests")
                            );
                            let git = session_meta.git.expect("expected git info");
                            assert_eq!(git.commit_hash.as_deref(), Some("abc123"));
                            assert_eq!(git.branch.as_deref(), Some("main"));
                            assert_eq!(
                                git.repository_url.as_deref(),
                                Some("https://example.com/repo.git")
                            );
                        }
                        "without_git" => {
                            assert!(session_meta.meta.instructions.is_none());
                            assert!(session_meta.git.is_none());
                        }
                        _ => unreachable!(),
                    }
                }
                other => panic!("case {case} parsed as unexpected item {other:?}"),
            }
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn deserialize_rollout_response_item_lines() {
        let timestamp = "2025-01-02T03:04:05.678Z";
        let cases: Vec<(&str, Value)> = vec![
            (
                "message",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "id": "legacy-message",
                        "role": "assistant",
                        "content": [
                            { "type": "output_text", "text": "Hello from assistant" }
                        ]
                    }
                }),
            ),
            (
                "reasoning",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "reasoning",
                        "id": "reasoning-1",
                        "summary": [
                            { "type": "summary_text", "text": "Summarized thoughts" }
                        ],
                        "content": [
                            { "type": "reasoning_text", "text": "Detailed reasoning" }
                        ],
                        "encrypted_content": "encrypted"
                    }
                }),
            ),
            (
                "local_shell_call",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "local_shell_call",
                        "id": "legacy-shell-call",
                        "call_id": "shell-call-1",
                        "status": "completed",
                        "action": {
                            "type": "exec",
                            "command": ["ls", "-la"],
                            "timeout_ms": 1200,
                            "working_directory": "/workspace",
                            "env": { "PATH": "/usr/bin" },
                            "user": "codex"
                        }
                    }
                }),
            ),
            (
                "function_call",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "id": "legacy-function",
                        "name": "shell",
                        "arguments": "{\"command\":[\"echo\",\"hi\"]}",
                        "call_id": "call-123"
                    }
                }),
            ),
            (
                "function_call_output",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "function_call_output",
                        "call_id": "call-123",
                        "output": "{\"stdout\":\"done\"}"
                    }
                }),
            ),
            (
                "custom_tool_call",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "custom_tool_call",
                        "id": "legacy-tool",
                        "status": "completed",
                        "call_id": "tool-456",
                        "name": "my_tool",
                        "input": "{\"foo\":1}"
                    }
                }),
            ),
            (
                "custom_tool_call_output",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "custom_tool_call_output",
                        "call_id": "tool-456",
                        "output": "tool finished"
                    }
                }),
            ),
            (
                "web_search_call",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "web_search_call",
                        "id": "legacy-search",
                        "status": "completed",
                        "action": {
                            "type": "search",
                            "query": "weather in SF"
                        }
                    }
                }),
            ),
            (
                "other",
                json!({
                    "timestamp": timestamp,
                    "type": "response_item",
                    "payload": {
                        "type": "new_future_item",
                        "foo": "bar"
                    }
                }),
            ),
        ];

        for (case, value) in cases {
            let parsed = parse_rollout_line(value, case);
            assert_eq!(parsed.timestamp, timestamp);
            match (case, parsed.item) {
                (
                    "message",
                    RolloutItem::ResponseItem(ResponseItem::Message { role, content, .. }),
                ) => {
                    assert_eq!(role, "assistant");
                    assert_eq!(content.len(), 1);
                    if let ContentItem::OutputText { text } = &content[0] {
                        assert_eq!(text, "Hello from assistant");
                    } else {
                        panic!("unexpected content variant in message case");
                    }
                }
                (
                    "reasoning",
                    RolloutItem::ResponseItem(ResponseItem::Reasoning {
                        summary,
                        content,
                        encrypted_content,
                        ..
                    }),
                ) => {
                    assert_eq!(summary.len(), 1);
                    match &summary[0] {
                        ReasoningItemReasoningSummary::SummaryText { text } => {
                            assert_eq!(text, "Summarized thoughts");
                        }
                        other => panic!("unexpected summary variant: {other:?}"),
                    }
                    let reasoning_content = content.expect("expected reasoning content");
                    assert_eq!(reasoning_content.len(), 1);
                    if let ReasoningItemContent::ReasoningText { text } = &reasoning_content[0] {
                        assert_eq!(text, "Detailed reasoning");
                    } else {
                        panic!("unexpected reasoning content variant");
                    }
                    assert_eq!(encrypted_content.as_deref(), Some("encrypted"));
                }
                (
                    "local_shell_call",
                    RolloutItem::ResponseItem(ResponseItem::LocalShellCall {
                        call_id,
                        status,
                        action,
                        ..
                    }),
                ) => {
                    assert_eq!(call_id.as_deref(), Some("shell-call-1"));
                    assert!(matches!(status, LocalShellStatus::Completed));
                    match action {
                        LocalShellAction::Exec(exec) => {
                            assert_eq!(exec.command, vec!["ls", "-la"]);
                            assert_eq!(exec.timeout_ms, Some(1200));
                            assert_eq!(exec.working_directory.as_deref(), Some("/workspace"));
                            let env = exec.env.expect("expected env map");
                            assert_eq!(env.get("PATH"), Some(&"/usr/bin".to_string()));
                            assert_eq!(exec.user.as_deref(), Some("codex"));
                        }
                    }
                }
                (
                    "function_call",
                    RolloutItem::ResponseItem(ResponseItem::FunctionCall {
                        name,
                        arguments,
                        call_id,
                        ..
                    }),
                ) => {
                    assert_eq!(name, "shell");
                    assert_eq!(arguments, "{\"command\":[\"echo\",\"hi\"]}");
                    assert_eq!(call_id, "call-123");
                }
                (
                    "function_call_output",
                    RolloutItem::ResponseItem(ResponseItem::FunctionCallOutput { output, .. }),
                ) => {
                    assert_eq!(output.content, "{\"stdout\":\"done\"}");
                    assert!(output.success.is_none());
                }
                (
                    "custom_tool_call",
                    RolloutItem::ResponseItem(ResponseItem::CustomToolCall {
                        status,
                        call_id,
                        name,
                        input,
                        ..
                    }),
                ) => {
                    assert_eq!(status.as_deref(), Some("completed"));
                    assert_eq!(call_id, "tool-456");
                    assert_eq!(name, "my_tool");
                    assert_eq!(input, "{\"foo\":1}");
                }
                (
                    "custom_tool_call_output",
                    RolloutItem::ResponseItem(ResponseItem::CustomToolCallOutput {
                        output, ..
                    }),
                ) => {
                    assert_eq!(output, "tool finished");
                }
                (
                    "web_search_call",
                    RolloutItem::ResponseItem(ResponseItem::WebSearchCall {
                        status, action, ..
                    }),
                ) => {
                    assert_eq!(status.as_deref(), Some("completed"));
                    match action {
                        WebSearchAction::Search { query } => {
                            assert_eq!(query, "weather in SF");
                        }
                        WebSearchAction::Other => panic!("unexpected web search action variant"),
                    }
                }
                ("other", RolloutItem::ResponseItem(ResponseItem::Other)) => {}
                (case, item) => panic!("case {case} returned unexpected item {item:?}"),
            }
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn deserialize_rollout_event_msg_lines() {
        let timestamp = "2025-01-02T03:04:05.678Z";
        let cases: Vec<(&str, Value)> = vec![
            (
                "user_message",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "user_message",
                        "message": "Please help",
                        "kind": "plain",
                        "images": ["data:image/png;base64,AAA"]
                    }
                }),
            ),
            (
                "agent_message",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "agent_message",
                        "message": "Sure thing"
                    }
                }),
            ),
            (
                "agent_reasoning",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "agent_reasoning",
                        "text": "Thinking..."
                    }
                }),
            ),
            (
                "agent_reasoning_raw_content",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "agent_reasoning_raw_content",
                        "text": "raw reasoning"
                    }
                }),
            ),
            (
                "token_count_info",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 120,
                                "cached_input_tokens": 10,
                                "output_tokens": 30,
                                "reasoning_output_tokens": 5,
                                "total_tokens": 165
                            },
                            "last_token_usage": {
                                "input_tokens": 20,
                                "cached_input_tokens": 0,
                                "output_tokens": 15,
                                "reasoning_output_tokens": 5,
                                "total_tokens": 40
                            },
                            "model_context_window": 16000
                        }
                    }
                }),
            ),
            (
                "token_count_none",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": null
                    }
                }),
            ),
            (
                "entered_review_mode",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "entered_review_mode",
                        "prompt": "Need review",
                        "user_facing_hint": "double-check work"
                    }
                }),
            ),
            (
                "exited_review_mode",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "exited_review_mode",
                        "review_output": {
                            "findings": [
                                {
                                    "title": "Bug",
                                    "body": "Found an issue",
                                    "confidence_score": 0.4,
                                    "priority": 1,
                                    "code_location": {
                                        "absolute_file_path": "/workspace/src/lib.rs",
                                        "line_range": { "start": 1, "end": 3 }
                                    }
                                }
                            ],
                            "overall_correctness": "needs_changes",
                            "overall_explanation": "Please fix",
                            "overall_confidence_score": 0.9
                        }
                    }
                }),
            ),
            (
                "turn_aborted",
                json!({
                    "timestamp": timestamp,
                    "type": "event_msg",
                    "payload": {
                        "type": "turn_aborted",
                        "reason": "interrupted"
                    }
                }),
            ),
        ];

        for (case, value) in cases {
            let parsed = parse_rollout_line(value, case);
            assert_eq!(parsed.timestamp, timestamp);
            match (case, parsed.item) {
                ("user_message", RolloutItem::EventMsg(EventMsg::UserMessage(event))) => {
                    assert_eq!(event.message, "Please help");
                    assert!(matches!(event.kind, Some(InputMessageKind::Plain)));
                    let images = event.images.expect("expected images");
                    assert_eq!(images, vec!["data:image/png;base64,AAA".to_string()]);
                }
                ("agent_message", RolloutItem::EventMsg(EventMsg::AgentMessage(event))) => {
                    assert_eq!(event.message, "Sure thing");
                }
                ("agent_reasoning", RolloutItem::EventMsg(EventMsg::AgentReasoning(event))) => {
                    assert_eq!(event.text, "Thinking...");
                }
                (
                    "agent_reasoning_raw_content",
                    RolloutItem::EventMsg(EventMsg::AgentReasoningRawContent(event)),
                ) => {
                    assert_eq!(event.text, "raw reasoning");
                }
                ("token_count_info", RolloutItem::EventMsg(EventMsg::TokenCount(event))) => {
                    let info = event.info.expect("expected token info");
                    assert_eq!(info.total_token_usage.input_tokens, 120);
                    assert_eq!(info.total_token_usage.cached_input_tokens, 10);
                    assert_eq!(info.total_token_usage.output_tokens, 30);
                    assert_eq!(info.total_token_usage.reasoning_output_tokens, 5);
                    assert_eq!(info.total_token_usage.total_tokens, 165);
                    assert_eq!(info.last_token_usage.output_tokens, 15);
                    assert_eq!(info.model_context_window, Some(16000));
                }
                ("token_count_none", RolloutItem::EventMsg(EventMsg::TokenCount(event))) => {
                    assert!(event.info.is_none());
                }
                (
                    "entered_review_mode",
                    RolloutItem::EventMsg(EventMsg::EnteredReviewMode(request)),
                ) => {
                    assert_eq!(request.prompt, "Need review");
                    assert_eq!(request.user_facing_hint, "double-check work");
                }
                (
                    "exited_review_mode",
                    RolloutItem::EventMsg(EventMsg::ExitedReviewMode(event)),
                ) => {
                    let output = event.review_output.expect("expected review output");
                    assert_eq!(output.findings.len(), 1);
                    let finding = &output.findings[0];
                    assert_eq!(finding.title, "Bug");
                    assert_eq!(finding.body, "Found an issue");
                    assert_eq!(finding.confidence_score, 0.4);
                    assert_eq!(finding.priority, 1);
                    assert_eq!(
                        finding.code_location.absolute_file_path,
                        PathBuf::from("/workspace/src/lib.rs")
                    );
                    assert_eq!(finding.code_location.line_range.start, 1);
                    assert_eq!(finding.code_location.line_range.end, 3);
                    assert_eq!(output.overall_correctness, "needs_changes");
                    assert_eq!(output.overall_explanation, "Please fix");
                    assert_eq!(output.overall_confidence_score, 0.9);
                }
                ("turn_aborted", RolloutItem::EventMsg(EventMsg::TurnAborted(event))) => {
                    assert_eq!(event.reason, TurnAbortReason::Interrupted);
                }
                (case, item) => panic!("case {case} returned unexpected item {item:?}"),
            }
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn deserialize_rollout_misc_lines() {
        let timestamp = "2025-01-02T03:04:05.678Z";
        let cases: Vec<(&str, Value)> = vec![
            (
                "compacted",
                json!({
                    "timestamp": timestamp,
                    "type": "compacted",
                    "payload": {
                        "message": "Turn summary"
                    }
                }),
            ),
            (
                "turn_context_workspace",
                json!({
                    "timestamp": timestamp,
                    "type": "turn_context",
                    "payload": {
                        "cwd": "/workspace",
                        "approval_policy": "on-request",
                        "sandbox_policy": {
                            "mode": "workspace-write",
                            "writable_roots": ["/workspace/tmp"],
                            "network_access": true,
                            "exclude_tmpdir_env_var": false,
                            "exclude_slash_tmp": true
                        },
                        "model": "gpt-5",
                        "effort": "high",
                        "summary": "detailed"
                    }
                }),
            ),
            (
                "turn_context_read_only",
                json!({
                    "timestamp": timestamp,
                    "type": "turn_context",
                    "payload": {
                        "cwd": "/workspace",
                        "approval_policy": "never",
                        "sandbox_policy": {
                            "mode": "read-only"
                        },
                        "model": "gpt-5",
                        "summary": "auto"
                    }
                }),
            ),
        ];

        for (case, value) in cases {
            let parsed = parse_rollout_line(value, case);
            assert_eq!(parsed.timestamp, timestamp);
            match (case, parsed.item) {
                ("compacted", RolloutItem::Compacted(CompactedItem { message })) => {
                    assert_eq!(message, "Turn summary");
                }
                ("turn_context_workspace", RolloutItem::TurnContext(turn_context)) => {
                    assert_eq!(turn_context.cwd, PathBuf::from("/workspace"));
                    assert_eq!(turn_context.approval_policy, AskForApproval::OnRequest);
                    match turn_context.sandbox_policy {
                        SandboxPolicy::WorkspaceWrite {
                            writable_roots,
                            network_access,
                            exclude_tmpdir_env_var,
                            exclude_slash_tmp,
                        } => {
                            assert_eq!(writable_roots, vec![PathBuf::from("/workspace/tmp")]);
                            assert!(network_access);
                            assert!(!exclude_tmpdir_env_var);
                            assert!(exclude_slash_tmp);
                        }
                        other => panic!("expected workspace-write sandbox policy, got {other:?}"),
                    }
                    assert_eq!(turn_context.model, "gpt-5");
                    assert_eq!(turn_context.effort, Some(ReasoningEffortConfig::High));
                    assert_eq!(turn_context.summary, ReasoningSummaryConfig::Detailed);
                }
                ("turn_context_read_only", RolloutItem::TurnContext(turn_context)) => {
                    assert_eq!(turn_context.cwd, PathBuf::from("/workspace"));
                    assert_eq!(turn_context.approval_policy, AskForApproval::Never);
                    assert!(turn_context.effort.is_none());
                    assert_eq!(turn_context.summary, ReasoningSummaryConfig::Auto);
                    match turn_context.sandbox_policy {
                        SandboxPolicy::ReadOnly => {}
                        other => panic!("expected read-only sandbox policy, got {other:?}"),
                    }
                }
                (case, item) => panic!("case {case} returned unexpected item {item:?}"),
            }
        }
    }
}
