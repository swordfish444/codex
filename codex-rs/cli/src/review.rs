use clap::Parser;
use codex_common::CliConfigOverrides;
use codex_tui::Cli as TuiCli;
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, Parser)]
pub struct ReviewCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Source branch (defaults to current branch when omitted)
    #[arg(long = "source", short = 's', value_name = "BRANCH")]
    pub source: Option<String>,

    /// Target branch to review merging into
    #[arg(
        long = "target",
        short = 't',
        value_name = "BRANCH",
        visible_alias = "into",
        alias = "to",
        required = true
    )]
    pub target: String,
}

pub async fn run_review_command(
    cli: ReviewCommand,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> anyhow::Result<()> {
    let ReviewCommand {
        mut config_overrides,
        source,
        target,
    } = cli;
    let source = match source {
        Some(s) => s,
        None => current_branch().await?,
    };
    let target = target;

    // Compute the patch representing merging `source` into `target`.
    // Use the triple-dot diff to compare source against the merge base with target.
    let patch = git_diff_patch(&target, &source).await?;

    if patch.trim().is_empty() {
        eprintln!("No differences found between {source} and {target}.");
        return Ok(());
    }

    let prompt = build_review_prompt(&source, &target, &patch);

    // Launch the interactive TUI with the constructed prompt, like normal codex.
    let tui_cli = TuiCli {
        prompt: Some(prompt),
        images: vec![],
        model: None,
        oss: false,
        config_profile: None,
        sandbox_mode: None,
        approval_policy: None,
        full_auto: false,
        dangerously_bypass_approvals_and_sandbox: false,
        cwd: None,
        config_overrides: std::mem::take(&mut config_overrides),
    };

    let usage = codex_tui::run_main(tui_cli, codex_linux_sandbox_exe).await?;
    if !usage.is_zero() {
        println!("{}", codex_core::protocol::FinalOutput::from(usage));
    }
    Ok(())
}

fn build_review_prompt(source: &str, target: &str, patch: &str) -> String {
    let header = format!("You are reviewing a proposed merge of '{source}' into '{target}'.");
    let instructions = "Please analyze the following git diff patch in detail. Provide: \n\
        - 4 specific areas of improvement (actionable suggestions with rationale).\n\
        - 4 thoughtful questions a senior software engineer would ask.\n\
        Consider correctness, performance, security, readability, testing, and documentation.\n\
        Structure the output with two sections: 'Areas of Improvement' (numbered 1-4) and 'Questions' (numbered 1-4).";
    // Truncate the patch to ~80k tokens (approximate via 4 chars per token)
    let (truncated_patch, was_truncated) = truncate_patch_to_tokens(patch, 80_000);
    let diff_block = if was_truncated {
        format!("```patch\n{truncated_patch}\n...\n```\n")
    } else {
        format!("```patch\n{truncated_patch}\n```\n")
    };
    format!("{header}\n\n{instructions}\n\n{diff_block}")
}

fn truncate_patch_to_tokens(patch: &str, max_tokens: usize) -> (String, bool) {
    // Heuristic: ~4 characters per token
    let max_chars = max_tokens.saturating_mul(4);
    let mut count = 0usize;
    if patch.chars().count() <= max_chars {
        return (patch.to_string(), false);
    }
    let mut out = String::with_capacity(max_chars);
    for ch in patch.chars() {
        if count >= max_chars {
            break;
        }
        out.push(ch);
        count += 1;
    }
    (out, true)
}

async fn git_diff_patch(target: &str, source: &str) -> anyhow::Result<String> {
    // git diff target...source shows changes on source since the merge base with target.
    let output = Command::new("git")
        .arg("-c")
        .arg("color.ui=never")
        .arg("diff")
        .arg("--no-ext-diff")
        .arg("--patch")
        .arg("--binary")
        .arg("-M")
        .arg("-C")
        .arg(format!("{target}...{source}"))
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to compute git diff: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn current_branch() -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to determine current branch: {stderr}");
    }
    let mut name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() || name == "HEAD" {
        name = "HEAD".to_string();
    }
    Ok(name)
}
