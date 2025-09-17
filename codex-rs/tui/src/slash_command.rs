use std::str::FromStr;
use strum::IntoEnumIterator;
use strum_macros::AsRefStr;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

use codex_core::protocol::Op;
use codex_core::protocol::ReviewRequest;

/// Commands that can be invoked by starting a message with a leading slash.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    Model,
    Approvals,
    Review,
    New,
    Init,
    Compact,
    Diff,
    Mention,
    Status,
    Mcp,
    Logout,
    Quit,
    #[cfg(debug_assertions)]
    TestApproval,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::New => "start a new chat during a conversation",
            SlashCommand::Init => "create an AGENTS.md file with instructions for Codex",
            SlashCommand::Compact => "summarize conversation to prevent hitting the context limit",
            SlashCommand::Review => "review my current changes and find issues",
            SlashCommand::Quit => "exit Codex",
            SlashCommand::Diff => "show git diff (including untracked files)",
            SlashCommand::Mention => "mention a file",
            SlashCommand::Status => "show current session configuration and token usage",
            SlashCommand::Model => "choose what model and reasoning effort to use",
            SlashCommand::Approvals => "choose what Codex can do without approval",
            SlashCommand::Mcp => "list configured MCP tools",
            SlashCommand::Logout => "log out of Codex",
            #[cfg(debug_assertions)]
            SlashCommand::TestApproval => "test approval request",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }

    /// Whether this command can be run while a task is in progress.
    pub fn available_during_task(self) -> bool {
        match self {
            SlashCommand::New
            | SlashCommand::Init
            | SlashCommand::Compact
            | SlashCommand::Model
            | SlashCommand::Approvals
            | SlashCommand::Review
            | SlashCommand::Logout => false,
            SlashCommand::Diff
            | SlashCommand::Mention
            | SlashCommand::Status
            | SlashCommand::Mcp
            | SlashCommand::Quit => true,

            #[cfg(debug_assertions)]
            SlashCommand::TestApproval => true,
        }
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    SlashCommand::iter().map(|c| (c.command(), c)).collect()
}

/// Input mode for a slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashInputMode {
    /// Execute immediately (handled via popup selection or dispatch_command).
    Immediate,
    /// Prefill composer with `/<cmd> <default_prompt>`; on Enter, submit a specific Op.
    Compose { default_prompt: &'static str },
}

/// Describe how a built-in command is edited/submitted.
pub fn slash_input_mode(cmd: SlashCommand) -> SlashInputMode {
    match cmd {
        SlashCommand::Review => SlashInputMode::Compose {
            default_prompt: "Review my current changes.",
        },
        _ => SlashInputMode::Immediate,
    }
}

/// If `text` begins with a built-in slash command, return the command and the
/// remainder (everything after the command token, across newlines). Callers that
/// only want to consider the first line can pass just that slice.
pub fn parse_slash_invocation(text: &str) -> Option<(SlashCommand, &str)> {
    // Must start with a slash.
    let after_slash = text.strip_prefix('/')?;
    // Allow optional whitespace after the slash before the command token.
    let token_start = after_slash.trim_start();
    let mut parts = token_start.splitn(2, char::is_whitespace);
    let cmd_token = parts.next()?;
    if cmd_token.is_empty() {
        return None;
    }
    let cmd = SlashCommand::from_str(cmd_token).ok()?;
    // Preserve the rest of the original input (including newlines),
    // trimming only leading/trailing whitespace around it.
    let remainder = parts.next().map(str::trim).unwrap_or("");
    Some((cmd, remainder))
}

/// Map a compose-style command + remainder into an Op for Codex.
/// Returns None for commands that don't have a direct Op mapping.
pub fn slash_submit_op(cmd: SlashCommand, remainder: String) -> Option<Op> {
    match cmd {
        SlashCommand::Review => Some(Op::Review {
            review_request: ReviewRequest {
                prompt: remainder.clone(),
                user_facing_hint: remainder,
            },
        }),
        _ => None,
    }
}
