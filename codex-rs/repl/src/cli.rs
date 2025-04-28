use clap::ArgAction;
use clap::Parser;
use clap::ValueEnum;
use codex_core::ApprovalModeCliArg;
use codex_core::SandboxModeCliArg;
use std::path::PathBuf;

/// Command‑line arguments.
/// Command-line interface for the interactive `codex-repl` agent.
#[derive(Debug, Parser, Clone)]
#[command(
    author,
    version,
    about = "Interactive Codex CLI that streams all agent actions."
)]
pub struct Cli {
    /// User prompt to start the session.
    pub prompt: Option<String>,

    /// Override the default model from ~/.codex/config.toml.
    #[arg(short, long)]
    pub model: Option<String>,

    /// Optional images to attach to the prompt.
    #[arg(long, value_name = "FILE")]
    pub images: Vec<PathBuf>,

    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    ///
    /// The flag may be passed up to three times. Without any -v the CLI only prints warnings and errors.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Don't use colored ansi output for verbose logging
    #[arg(long)]
    pub no_ansi: bool,

    /// Configure when the model requires human approval before executing a command.
    #[arg(long = "ask-for-approval", short = 'a')]
    pub approval_policy: Option<ApprovalModeCliArg>,

    /// Configure the process restrictions when a command is executed.
    ///
    /// Uses OS-specific sandboxing tools; Seatbelt on OSX, landlock+seccomp on Linux.
    #[arg(long = "sandbox", short = 's')]
    pub sandbox_policy: Option<SandboxModeCliArg>,

    /// Allow running Codex outside a Git repository.  By default the CLI
    /// aborts early when the current working directory is **not** inside a
    /// Git repo because most agents rely on `git` for interacting with the
    /// code‑base.  Pass this flag if you really know what you are doing.
    #[arg(long, action = ArgAction::SetTrue, default_value_t = false)]
    pub allow_no_git_exec: bool,

    /// Disable server‑side response storage (sends the full conversation context with every request)
    #[arg(long = "disable-response-storage", default_value_t = false)]
    pub disable_response_storage: bool,

    /// Record submissions into file as JSON
    #[arg(short = 'S', long)]
    pub record_submissions: Option<PathBuf>,

    /// Record events into file as JSON
    #[arg(short = 'E', long)]
    pub record_events: Option<PathBuf>,
}

impl Cli {
    /// This is effectively the opposite of Clap; we want the ability to take
    /// a structured `Cli` object, and then pass it to a binary as argv[].
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![];

        if let Some(model) = &self.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        for img in &self.images {
            args.push("--image".into());
            args.push(img.to_string_lossy().into_owned());
        }

        if self.no_ansi {
            args.push("--no-ansi".into());
        }

        for _ in 0..self.verbose {
            args.push("-v".into());
        }

        args.push("--ask-for-approval".into());
        args.push(
            self.approval_policy
                .to_possible_value()
                .expect("foo")
                .get_name()
                .to_string(),
        );

        args.push("--sandbox".into());
        args.push(
            self.sandbox_policy
                .to_possible_value()
                .expect("foo")
                .get_name()
                .to_string(),
        );

        if self.allow_no_git_exec {
            args.push("--allow-no-git-exec".into());
        }

        if self.disable_response_storage {
            args.push("--disable-response-storage".into());
        }

        if let Some(path) = &self.record_submissions {
            args.push("--record-submissions".into());
            args.push(path.to_string_lossy().into_owned());
        }

        if let Some(path) = &self.record_events {
            args.push("--record-events".into());
            args.push(path.to_string_lossy().into_owned());
        }

        if let Some(prompt) = &self.prompt {
            args.push(prompt.clone());
        }
        args
    }
}
