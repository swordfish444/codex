use std::collections::HashMap;

use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter, EnumString};

/// Commands that can be invoked by starting a message with a leading slash.
///
/// The `strum` derives ensure we get for free:
///  * `FromStr` parsing (`EnumString`)
///  * iteration over all variants (`EnumIter`)
///  * kebab-case string representation via `AsRefStr` (configured with
///    `serialize_all = "kebab-case"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    Help,
    Clear,
    Reset,
    Exit,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::Help => "Show this help message.",
            SlashCommand::Clear => "Clear the chat history.",
            SlashCommand::Reset => "Reset the chat history.",
            SlashCommand::Exit => "Exit the application.",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        match self {
            SlashCommand::Help => "help",
            SlashCommand::Clear => "clear",
            SlashCommand::Reset => "reset",
            SlashCommand::Exit => "exit",
        }
    }
}

/// Return all built-in commands in a HashMap keyed by their command string.
pub fn built_in_slash_commands() -> HashMap<&'static str, SlashCommand> {
    SlashCommand::iter().map(|c| (c.command(), c)).collect()
}
