mod prompt_override;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;
use tokio::io::{self};
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::sync::watch;

use crate::auth::AuthManager;
use crate::codex::INITIAL_SUBMIT_ID;
use crate::codex_conversation::CodexConversation;
use crate::config::Config;
use crate::config::ConfigOverrides;
use crate::conversation_manager::ConversationManager;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::SessionConfiguredEvent;
use codex_protocol::protocol::Submission;
use tracing::debug;
use tracing::error;
use tracing::info;

pub use prompt_override::load_system_prompt_override;

/// Sample Python harness script that can be used to drive the prompt harness binary.
pub const SAMPLE_DRIVER: &str = include_str!("driver.py");

#[derive(Debug, Clone)]
pub struct PromptHarnessCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PromptHarnessOptions {
    pub cli_overrides: Vec<(String, toml::Value)>,
    pub prompt_file: PathBuf,
    pub command: PromptHarnessCommand,
}

/// Load configuration, override system prompt, and execute the harness.
pub async fn run_prompt_harness(opts: PromptHarnessOptions) -> Result<()> {
    let PromptHarnessOptions {
        cli_overrides,
        prompt_file,
        command,
    } = opts;

    let base_instructions = load_system_prompt_override(&prompt_file).with_context(|| {
        format!(
            "failed to load system prompt override from {}",
            prompt_file.display()
        )
    })?;

    let config = load_config(cli_overrides, base_instructions.clone())?;
    let auth_manager = AuthManager::shared(config.codex_home.clone());
    let conversation_manager = ConversationManager::new(auth_manager);

    let session = conversation_manager
        .new_conversation(config)
        .await
        .context("failed to start Codex conversation")?;

    info!(
        ?command.program,
        args = ?command.args,
        "starting prompt harness child process"
    );

    run_conversation(command, session.conversation, session.session_configured).await
}

fn load_config(
    cli_overrides: Vec<(String, toml::Value)>,
    base_instructions: String,
) -> Result<Config> {
    let overrides = ConfigOverrides {
        base_instructions: Some(base_instructions.clone()),
        ..ConfigOverrides::default()
    };
    let mut config = Config::load_with_cli_overrides(cli_overrides, overrides)?;
    let effective_instructions = config
        .base_instructions
        .clone()
        .unwrap_or(base_instructions);
    config.model_family.base_instructions = effective_instructions.clone();
    // Force the override to be the only set of instructions that the model sees.
    config.user_instructions = Some(effective_instructions);

    Ok(config)
}

async fn run_conversation(
    command: PromptHarnessCommand,
    conversation: Arc<CodexConversation>,
    session_configured: SessionConfiguredEvent,
) -> Result<()> {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn child process `{}`",
                command.program.display()
            )
        })?;

    let child_stdin = child
        .stdin
        .take()
        .context("child process lacks stdin pipe")?;
    let child_stdout = child
        .stdout
        .take()
        .context("child process lacks stdout pipe")?;

    let (child_exit_tx, child_exit_rx) = watch::channel(false);

    let events_task = tokio::spawn(pump_events(
        conversation.clone(),
        session_configured,
        child_stdin,
        child_exit_rx.clone(),
    ));

    let submissions_task = tokio::spawn(pump_submissions(
        conversation,
        child_stdout,
        child_exit_rx.clone(),
    ));

    let status = child
        .wait()
        .await
        .with_context(|| format!("failed to wait for child `{}`", command.program.display()))?;
    let _ = child_exit_tx.send(true);

    info!(?status, "prompt harness child exited");

    match events_task.await {
        Ok(res) => res?,
        Err(err) => return Err(err).context("event pump task panicked"),
    }

    match submissions_task.await {
        Ok(res) => res?,
        Err(err) => return Err(err).context("submission pump task panicked"),
    }

    Ok(())
}

async fn pump_events(
    conversation: Arc<CodexConversation>,
    session_configured: SessionConfiguredEvent,
    child_stdin: ChildStdin,
    mut child_exit_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut writer = BufWriter::new(child_stdin);

    let initial_event = Event {
        id: INITIAL_SUBMIT_ID.to_string(),
        msg: EventMsg::SessionConfigured(session_configured),
    };

    if !write_event(&mut writer, &initial_event).await? {
        return Ok(());
    }

    loop {
        tokio::select! {
            changed = child_exit_rx.changed() => {
                if changed.is_err() || *child_exit_rx.borrow() {
                    break;
                }
            }
            event = conversation.next_event() => {
                let event = event?;
                if !write_event(&mut writer, &event).await? {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn pump_submissions(
    conversation: Arc<CodexConversation>,
    child_stdout: ChildStdout,
    mut child_exit_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut reader = BufReader::new(child_stdout).lines();

    loop {
        tokio::select! {
            changed = child_exit_rx.changed() => {
                if changed.is_err() || *child_exit_rx.borrow() {
                    break;
                }
            }
            line = reader.next_line() => {
                match line? {
                    Some(line) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Submission>(trimmed) {
                            Ok(submission) => {
                                if let Err(err) = conversation.submit_with_id(submission).await {
                                    return Err(err.into());
                                }
                            }
                            Err(err) => {
                                if trimmed.starts_with('{') || trimmed.starts_with('[') {
                                    error!("invalid submission from child: {err}");
                                } else {
                                    debug!("ignoring non-JSON child output line: {trimmed}");
                                }
                            }
                        }
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}

async fn write_event(writer: &mut BufWriter<ChildStdin>, event: &Event) -> Result<bool> {
    let json = serde_json::to_string(event).context("failed to serialize event")?;
    let write_res = writer.write_all(json.as_bytes()).await;
    if let Err(err) = write_res {
        return handle_broken_pipe(err);
    }
    let newline_res = writer.write_all(b"\n").await;
    if let Err(err) = newline_res {
        return handle_broken_pipe(err);
    }
    let flush_res = writer.flush().await;
    if let Err(err) = flush_res {
        return handle_broken_pipe(err);
    }
    Ok(true)
}

fn handle_broken_pipe(err: io::Error) -> Result<bool> {
    match err.kind() {
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::NotConnected => {
            info!("child process closed stdin");
            Ok(false)
        }
        _ => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn set_env_var(key: &str, value: impl AsRef<std::ffi::OsStr>) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    struct EnvVarReset<'a> {
        key: &'a str,
        prev: Option<String>,
    }

    impl<'a> EnvVarReset<'a> {
        fn new(key: &'a str) -> Self {
            let prev = std::env::var(key).ok();
            Self { key, prev }
        }
    }

    impl Drop for EnvVarReset<'_> {
        fn drop(&mut self) {
            if let Some(prev) = &self.prev {
                set_env_var(self.key, prev);
            } else {
                remove_env_var(self.key);
            }
        }
    }

    #[test]
    fn load_config_applies_base_instructions() {
        let _guard = ENV_GUARD.lock().expect("lock env guard");
        let codex_home = TempDir::new().expect("create codex home");
        let _reset = EnvVarReset::new("CODEX_HOME");
        set_env_var("CODEX_HOME", codex_home.path());

        let file = NamedTempFile::new().expect("create temp");
        std::fs::write(file.path(), "prompt override").expect("write prompt");

        let base = load_system_prompt_override(file.path()).expect("load prompt");
        let config = load_config(Vec::new(), base.clone()).expect("load config");
        assert_eq!(config.base_instructions.as_deref(), Some(base.as_str()));
        assert_eq!(config.user_instructions.as_deref(), Some(base.as_str()));
        assert_eq!(config.model_family.base_instructions, base);
    }
}
