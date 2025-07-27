use crate::slash_command::SlashCommand;
use crate::at_command::AtCommand;
use crate::bottom_pane::CommandInfo;

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Slash(SlashCommand),
    At(AtCommand),
}

impl CommandInfo for Command {
    fn command(&self) -> &'static str {
        match self { Command::Slash(s) => s.command(), Command::At(a) => a.command() }
    }
    fn description(&self) -> &'static str {
        match self { Command::Slash(s) => s.description(), Command::At(a) => a.description() }
    }
} 