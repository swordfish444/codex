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
    /// Generate a concise summary of the current conversation and replace the
    /// history with that summary so you can continue with a fresh context.
    Compact,
    Diff,
    Quit,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::New => "Start a new chat.",
            SlashCommand::Compact => "Clear conversation history but keep a summary in context.",
            SlashCommand::Quit => "Exit the application.",
            SlashCommand::Diff => {
                "Show git diff of the working directory (including untracked files)"
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
    use super::*;

    #[test]
    fn menu_includes_compact() {
        let cmds = built_in_slash_commands();
        let names: Vec<&str> = cmds.iter().map(|(n, _)| *n).collect();
        assert!(
            names.contains(&"compact"),
            "/compact must be present in the slash menu"
        );
    }
}
