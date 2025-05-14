use std::collections::HashMap;

/// Command that can be invoked via the composer by starting the message with a
/// slash followed by the command name.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// Command name without the leading slash.
    command: &'static str,

    /// Command description suitable for display in the UI.
    description: &'static str,
}

impl SlashCommand {
    /// Return the command string without the leading slash.
    pub fn command(&self) -> &str {
        self.command
    }

    /// Return the human-readable description for the command.
    pub fn description(&self) -> &str {
        self.description
    }
}

pub fn built_in_slash_commands() -> HashMap<String, SlashCommand> {
    vec![
        SlashCommand {
            command: "help",
            description: "Show this help message.",
        },
        SlashCommand {
            command: "clear",
            description: "Clear the chat history.",
        },
        SlashCommand {
            command: "reset",
            description: "Reset the chat history.",
        },
        SlashCommand {
            command: "exit",
            description: "Exit the application.",
        },
    ]
    .into_iter()
    .map(|cmd| (cmd.command.to_owned(), cmd))
    .collect::<HashMap<String, SlashCommand>>()
}
