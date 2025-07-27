use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum AtCommand {
    // Order is presentation order in @ popup.
    ClipboardImage, // import image from clipboard
    File,  // open file search popup
}

impl AtCommand {
    pub fn description(self) -> &'static str {
        match self {
            AtCommand::ClipboardImage => "Import an image from the system clipboard (can be used with ctrl+v).",
            AtCommand::File => "Search for a file to insert its path.",
        }
    }
    pub fn command(self) -> &'static str { self.into() }
}

pub fn built_in_at_commands() -> Vec<(&'static str, AtCommand)> {
    AtCommand::iter().map(|c| (c.command(), c)).collect()
} 