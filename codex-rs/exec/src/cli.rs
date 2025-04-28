use clap::Parser;
use codex_core::SandboxModeCliArg;
use std::path::PathBuf;

/// Command-line interface for the non-interactive `codex-exec` agent.
///
#[derive(Parser, Debug, Clone)]
#[command(version)]
pub struct Cli {
    /// Optional image(s) to attach to the initial prompt.
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
    pub images: Vec<PathBuf>,

    /// Model the agent should use.
    #[arg(long, short = 'm')]
    pub model: Option<String>,

    /// Configure the process restrictions when a command is executed.
    ///
    /// Uses OS-specific sandboxing tools; Seatbelt on OSX, landlock+seccomp on Linux.
    #[arg(long = "sandbox", short = 's')]
    pub sandbox_policy: Option<SandboxModeCliArg>,

    /// Allow running Codex outside a Git repository.
    #[arg(long = "skip-git-repo-check", default_value_t = false)]
    pub skip_git_repo_check: bool,

    /// Disable server‑side response storage (sends the full conversation context with every request)
    #[arg(long = "disable-response-storage", default_value_t = false)]
    pub disable_response_storage: bool,

    /// Initial instructions for the agent.
    pub prompt: Option<String>,
}

impl Cli {
    /// This is effectively the opposite of Clap; we want the ability to take
    /// a structured `Cli` object, and then pass it to a binary as argv[].
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        for img in &self.images {
            args.push("--image".into());
            args.push(img.to_string_lossy().into_owned());
        }

        if let Some(model) = &self.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        if self.skip_git_repo_check {
            args.push("--skip-git-repo-check".into());
        }

        if self.disable_response_storage {
            args.push("--disable-response-storage".into());
        }

        if let Some(prompt) = &self.prompt {
            args.push(prompt.clone());
        }

        args
    }
}
