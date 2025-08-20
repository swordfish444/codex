use clap::Parser;
use codex_common::CliConfigOverrides;
use codex_exec::Cli as ExecCli;
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, Parser)]
pub struct ReviewCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Branch merge spec in the form: source->target
    #[arg(
        value_name = "MERGE",
        help = "Format: branch1->branch2 (source->target)",
        required_unless_present = "target"
    )]
    pub merge_spec: Option<String>,

    /// Source branch (defaults to current branch when omitted)
    #[arg(long = "source", short = 's', value_name = "BRANCH")]
    pub source: Option<String>,

    /// Target branch (alternative to MERGE positional)
    #[arg(
        long = "target",
        short = 't',
        value_name = "BRANCH",
        visible_alias = "into",
        alias = "to"
    )]
    pub target: Option<String>,
}

pub async fn run_review_command(
    cli: ReviewCommand,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> anyhow::Result<()> {
    let ReviewCommand {
        mut config_overrides,
        merge_spec,
        source,
        target,
    } = cli;

    let (source, target) = match (source, target, merge_spec) {
        (Some(s), Some(t), _) => (s, t),
        (maybe_s, Some(t), None) => {
            let s = match maybe_s {
                Some(s) => s,
                None => current_branch().await?,
            };
            (s, t)
        }
        (_, _, Some(spec)) => parse_merge_spec(&spec)?,
        _ => unreachable!("clap should enforce required arguments"),
    };

    // Compute the patch representing merging `source` into `target`.
    // Use the triple-dot diff to compare source against the merge base with target.
    let patch = git_diff_patch(&target, &source).await?;

    if patch.trim().is_empty() {
        eprintln!("No differences found between {source} and {target}.");
        return Ok(());
    }

    let prompt = build_review_prompt(&source, &target, &patch);

    // Reuse the non-interactive exec runner with our constructed prompt.
    // Build the default CLI via clap parsing to avoid reaching into private types.
    let mut exec_cli = ExecCli::parse_from(["codex-exec", &prompt]);
    exec_cli.config_overrides = std::mem::take(&mut config_overrides);

    codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await
}

fn build_review_prompt(source: &str, target: &str, patch: &str) -> String {
    let header = format!("You are reviewing a proposed merge of '{source}' into '{target}'.");
    let instructions = "Please analyze the following git diff patch in detail. Provide: \n\
        - 4 specific areas of improvement (actionable suggestions with rationale).\n\
        - 4 thoughtful questions a senior software engineer would ask.\n\
        Consider correctness, performance, security, readability, testing, and documentation.\n\
        Structure the output with two sections: 'Areas of Improvement' (numbered 1-4) and 'Questions' (numbered 1-4).";
    let diff_block = format!("```patch\n{patch}\n```\n");
    format!("{header}\n\n{instructions}\n\n{diff_block}")
}

fn parse_merge_spec(spec: &str) -> anyhow::Result<(String, String)> {
    let parts: Vec<&str> = spec.split("->").collect();
    if parts.len() != 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        anyhow::bail!("Invalid merge spec: {spec}. Expected format: source->target");
    }
    Ok((parts[0].trim().to_string(), parts[1].trim().to_string()))
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
