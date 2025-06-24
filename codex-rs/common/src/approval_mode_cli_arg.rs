//! Standard type to use with the `--approval-mode` CLI option.

use clap::ValueEnum;

use codex_core::protocol::AskForApproval;

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ApprovalModeCliArg {
    /// Run all commands without asking for user approval.  Only escalates when a command fails.
    OnFailure,

    /// Only run “known safe” commands (e.g. ls, cat, sed) automatically.
    UnlessAllowListed,

    /// Never ask for user approval; return execution failures directly to the model.
    Never,
}

impl From<ApprovalModeCliArg> for AskForApproval {
    fn from(value: ApprovalModeCliArg) -> Self {
        match value {
            ApprovalModeCliArg::OnFailure => AskForApproval::OnFailure,
            ApprovalModeCliArg::UnlessAllowListed => AskForApproval::UnlessAllowListed,
            ApprovalModeCliArg::Never => AskForApproval::Never,
        }
    }
}
