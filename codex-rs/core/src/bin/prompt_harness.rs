use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::ArgAction;
use clap::Parser;
use codex_core::prompt_harness::PromptHarnessCommand;
use codex_core::prompt_harness::PromptHarnessOptions;
use codex_core::prompt_harness::run_prompt_harness;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Run Codex with a system prompt override and attach a JSON protocol script."
)]
struct PromptHarnessCli {
    /// Override configuration values (`toml`-parsed). Repeatable.
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append
    )]
    raw_overrides: Vec<String>,

    /// Path to the file containing replacement system instructions for Codex.
    #[arg(long = "system-prompt-file", value_name = "FILE")]
    system_prompt_file: PathBuf,

    /// Command to execute. Receives Codex protocol events on stdin and must
    /// emit submissions as JSON on stdout.
    #[arg(
        value_name = "COMMAND",
        trailing_var_arg = true,
        default_values = ["python3", "core/src/prompt_harness/driver.py"]
    )]
    command: Vec<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = PromptHarnessCli::parse();
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();

    let overrides = parse_overrides(&cli.raw_overrides)?;
    let command = build_command(cli.command).context("command was missing program name")?;

    let options = PromptHarnessOptions {
        cli_overrides: overrides,
        prompt_file: cli.system_prompt_file,
        command,
    };

    run_prompt_harness(options).await
}

fn build_command(mut parts: Vec<String>) -> Option<PromptHarnessCommand> {
    if parts.is_empty() {
        return None;
    }
    let program = PathBuf::from(parts.remove(0));
    Some(PromptHarnessCommand {
        program,
        args: parts,
    })
}

fn parse_overrides(raw: &[String]) -> Result<Vec<(String, toml::Value)>> {
    raw.iter()
        .map(|entry| parse_single_override(entry))
        .collect()
}

fn parse_single_override(raw: &str) -> Result<(String, toml::Value)> {
    let mut split = raw.splitn(2, '=');
    let key = split
        .next()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| anyhow!("override missing key: {raw}"))?;
    let value = split
        .next()
        .map(str::trim)
        .ok_or_else(|| anyhow!("override missing '=' delimiter: {raw}"))?;

    let parsed =
        parse_toml_value(value).unwrap_or_else(|| toml::Value::String(trim_override_string(value)));

    Ok((key.to_string(), parsed))
}

fn trim_override_string(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed.trim_matches(|c| c == '\'' || c == '"').to_string()
}

fn parse_toml_value(raw: &str) -> Option<toml::Value> {
    let wrapped = format!("_value_ = {raw}");
    let mut table: toml::Table = toml::from_str(&wrapped).ok()?;
    table.remove("_value_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_literal() {
        let (k, v) = parse_single_override("model='o4'").expect("override");
        assert_eq!(k, "model");
        assert_eq!(v, toml::Value::String("o4".to_string()));
    }

    #[test]
    fn parses_json_literal() {
        let (k, v) = parse_single_override("numbers=[1,2]").expect("override");
        assert_eq!(k, "numbers");
        assert_eq!(
            v,
            toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Integer(2)])
        );
    }

    #[test]
    fn rejects_missing_key() {
        assert!(parse_single_override("=oops").is_err());
    }

    #[test]
    fn rejects_missing_value() {
        assert!(parse_single_override("model").is_err());
    }

    #[test]
    fn build_command_splits_program_and_args() {
        let cmd = build_command(vec!["python".to_string(), "-V".to_string()]).expect("command");
        assert_eq!(cmd.program, PathBuf::from("python"));
        assert_eq!(cmd.args, vec!["-V".to_string()]);
    }
}
