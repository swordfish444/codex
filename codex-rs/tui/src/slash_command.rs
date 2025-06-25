use std::collections::HashMap;

use strum::IntoEnumIterator;
use strum_macros::AsRefStr; // derive macro
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

/// Commands that can be invoked by starting a message with a leading slash.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    New,
    ToggleMouseMode,
    /// Launch the external editor to edit the current prompt draft.
    EditPrompt,
    Quit,
    /// Add a dynamic mount (host path → container path).
    MountAdd,
    /// Remove a dynamic mount by container path.
    MountRemove,
    /// Inspect sandbox and container environment (mounts, permissions, network).
    InspectEnv,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::New => "Start a new chat.",
            SlashCommand::ToggleMouseMode =>
                "Toggle mouse mode (enable for scrolling, disable for text selection)",
            SlashCommand::EditPrompt =>
                "Open external editor to edit the current prompt.",
            SlashCommand::Quit => "Exit the application.",
            SlashCommand::MountAdd => "Add a mount: host path → container path.",
            SlashCommand::MountRemove => "Remove a mount by container path.",
            SlashCommand::InspectEnv => "Inspect sandbox and container environment (mounts, permissions, network)",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }
}

/// Return all built-in commands in a HashMap keyed by their command string.
pub fn built_in_slash_commands() -> HashMap<&'static str, SlashCommand> {
    SlashCommand::iter().map(|c| (c.command(), c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_includes_inspect_env() {
        let commands = built_in_slash_commands();
        assert_eq!(commands.get("inspect-env"), Some(&SlashCommand::InspectEnv));
    }

    #[test]
    fn inspect_env_description_contains_keyword() {
        let desc = SlashCommand::InspectEnv.description();
        assert!(desc.contains("sandbox"), "description was: {}", desc);
    }
}
