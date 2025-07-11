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
pub enum SlashCommand {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    New,
    Diff,
    Quit,
    ToggleMouseMode,
    Compact,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::New => "Start a new chat.",
            SlashCommand::ToggleMouseMode => {
                "Toggle mouse mode (enable for scrolling, disable for text selection)"
            }
            SlashCommand::Quit => "Exit the application.",
            SlashCommand::Diff => {
                "Show git diff of the working directory (including untracked files)"
            }
            SlashCommand::Compact => {
                "Summarize and compact the current conversation to free up context."
            }
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    SlashCommand::iter().map(|c| (c.command(), c)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[test]
    fn test_compact_from_string() {
        let result = SlashCommand::from_str("compact").unwrap();
        assert_eq!(result, SlashCommand::Compact);
    }

    #[test]
    fn test_compact_in_built_in_commands() {
        let built_in = built_in_slash_commands();
        let compact_entry = built_in.iter().find(|(cmd, _)| *cmd == "compact");

        assert!(compact_entry.is_some());
        let (cmd, slash_cmd) = compact_entry.unwrap();
        assert_eq!(*cmd, "compact");
        assert_eq!(*slash_cmd, SlashCommand::Compact);
    }
}
