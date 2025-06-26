use codex_core::protocol::Event;
use crossterm::event::KeyEvent;

use crate::slash_command::SlashCommand;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    Redraw,

    KeyEvent(KeyEvent),

    /// Scroll event with a value representing the "scroll delta" as the net
    /// scroll up/down events within a short time window.
    Scroll(i32),

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(codex_core::protocol::Op),

    /// Latest formatted log line emitted by `tracing`.
    LatestLog(String),

    /// Dispatch a recognized slash command from the UI (composer) to the app
    /// layer so it can be handled centrally.
    /// Dispatch a recognized slash command from the UI (composer) to the app
    /// layer so it can be handled centrally (interactive dialog).
    DispatchCommand(SlashCommand),
    /// Inline mount-add DSL: raw argument string (`host=... container=... mode=...`).
    InlineMountAdd(String),
    /// Inline mount-remove DSL: raw argument string (`container=...`).
    InlineMountRemove(String),
    /// Inline inspect-env DSL: raw argument string (unused).
    InlineInspectEnv(String),
    /// Perform mount-add: create symlink and update sandbox policy.
    MountAdd {
        host: std::path::PathBuf,
        container: std::path::PathBuf,
        mode: String,
    },
    /// Perform mount-remove: remove symlink and update sandbox policy.
    MountRemove {
        container: std::path::PathBuf,
    },
    /// Notify that the on-disk config.toml has changed and present diff.
    ConfigReloadRequest(String),
    /// Apply the new on-disk config.toml.
    ConfigReloadApply,
    /// Ignore on-disk config.toml changes and continue with old config.
    ConfigReloadIgnore,
    /// Run an arbitrary shell command in the agent's container (from hotkey prompt).
    ShellCommand(String),
    /// Result of a previously-invoked shell command: call ID, stdout, stderr, and exit code.
    ShellCommandResult {
        call_id: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
}

impl PartialEq for AppEvent {
    fn eq(&self, other: &Self) -> bool {
        use AppEvent::*;
        match (self, other) {
            (CodexEvent(_), CodexEvent(_)) => true,
            (Redraw, Redraw) => true,
            (KeyEvent(a), KeyEvent(b)) => a == b,
            (Scroll(a), Scroll(b)) => a == b,
            (ExitRequest, ExitRequest) => true,
            (CodexOp(a), CodexOp(b)) => a == b,
            (LatestLog(a), LatestLog(b)) => a == b,
            (DispatchCommand(a), DispatchCommand(b)) => a == b,
            (InlineMountAdd(a), InlineMountAdd(b)) => a == b,
            (InlineMountRemove(a), InlineMountRemove(b)) => a == b,
            (InlineInspectEnv(a), InlineInspectEnv(b)) => a == b,
            (
                MountAdd {
                    host: h1,
                    container: c1,
                    mode: m1,
                },
                MountAdd {
                    host: h2,
                    container: c2,
                    mode: m2,
                },
            ) => h1 == h2 && c1 == c2 && m1 == m2,
            (MountRemove { container: c1 }, MountRemove { container: c2 }) => c1 == c2,
            (ConfigReloadRequest(a), ConfigReloadRequest(b)) => a == b,
            (ConfigReloadApply, ConfigReloadApply) => true,
            (ConfigReloadIgnore, ConfigReloadIgnore) => true,
            (ShellCommand(a), ShellCommand(b)) => a == b,
            (
                ShellCommandResult {
                    call_id: i1,
                    stdout: o1,
                    stderr: e1,
                    exit_code: x1,
                },
                ShellCommandResult {
                    call_id: i2,
                    stdout: o2,
                    stderr: e2,
                    exit_code: x2,
                },
            ) => i1 == i2 && o1 == o2 && e1 == e2 && x1 == x2,
            _ => false,
        }
    }
}
