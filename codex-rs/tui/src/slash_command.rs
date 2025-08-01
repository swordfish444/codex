use strum::IntoEnumIterator;
use strum_macros::AsRefStr;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

/// Commands that can be invoked by starting a message with a leading slash.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    New,
    Compact,
    Diff,
    Quit,
    #[cfg(debug_assertions)]
    TestApproval,
}

impl Command {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            Command::New => "Start a new chat.",
            Command::Compact => "Compact the chat history.",
            Command::Quit => "Exit the application.",
            Command::Diff => "Show git diff of the working directory (including untracked files)",
            #[cfg(debug_assertions)]
            Command::TestApproval => "Test approval request",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, Command)> {
    Command::iter().map(|c| (c.command(), c)).collect()
}
