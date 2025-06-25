use codex_core::protocol::Event;
use crossterm::event::KeyEvent;

use crate::slash_command::SlashCommand;

#[allow(clippy::large_enum_variant)]
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
}
    /// Notify that the on-disk config.toml has changed and present diff.
    ConfigReloadRequest(String),
    /// Apply the new on-disk config.toml.
    ConfigReloadApply,
    /// Ignore on-disk config.toml changes and continue with old config.
    ConfigReloadIgnore,
