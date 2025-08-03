mod cli;
mod event_processor;
mod event_processor_with_human_output;
mod event_processor_with_json_output;

use std::io::IsTerminal;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

pub use cli::Cli;
use codex_core::codex_wrapper::CodexConversation;
use codex_core::codex_wrapper::{self};
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config_types::SandboxMode;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::TaskCompleteEvent;
use codex_core::util::is_inside_git_repo;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_json_output::EventProcessorWithJsonOutput;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;

// ----- Ollama model discovery and pull helpers (CLI) -----
// These helpers are used when the user passes both --ollama and --model=<name>.
// We verify the requested model is recorded in config.toml or present on the
// local Ollama instance; if missing we will pull it (subject to an allowlist)
// and record it in config.toml without prompting.

async fn fetch_ollama_models(host_root: &str) -> Vec<String> {
    let tags_url = format!("{host_root}/api/tags");
    match reqwest::Client::new().get(&tags_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(val) => val
                .get("models")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn read_ollama_models_list(config_path: &std::path::Path) -> Vec<String> {
    match std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(toml::Value::Table(root)) => root
            .get("model_providers")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("ollama"))
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("models"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_ollama_models(config_path: &std::path::Path, models: &[String]) -> std::io::Result<()> {
    use toml::value::Table as TomlTable;
    let mut root_value = if let Ok(contents) = std::fs::read_to_string(config_path) {
        toml::from_str::<toml::Value>(&contents).unwrap_or(toml::Value::Table(TomlTable::new()))
    } else {
        toml::Value::Table(TomlTable::new())
    };

    if !matches!(root_value, toml::Value::Table(_)) {
        root_value = toml::Value::Table(TomlTable::new());
    }
    let root_tbl = match root_value.as_table_mut() {
        Some(t) => t,
        None => return Err(std::io::Error::other("invalid TOML root value")),
    };

    let mp_val = root_tbl
        .entry("model_providers".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    if !mp_val.is_table() {
        *mp_val = toml::Value::Table(TomlTable::new());
    }
    let mp_tbl = match mp_val.as_table_mut() {
        Some(t) => t,
        None => return Err(std::io::Error::other("invalid model_providers table")),
    };

    let ollama_val = mp_tbl
        .entry("ollama".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    if !ollama_val.is_table() {
        *ollama_val = toml::Value::Table(TomlTable::new());
    }
    let ollama_tbl = match ollama_val.as_table_mut() {
        Some(t) => t,
        None => return Err(std::io::Error::other("invalid ollama table")),
    };
    let arr = toml::Value::Array(
        models
            .iter()
            .map(|m| toml::Value::String(m.clone()))
            .collect(),
    );
    ollama_tbl.insert("models".to_string(), arr);

    let updated =
        toml::to_string_pretty(&root_value).map_err(|e| std::io::Error::other(e.to_string()))?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, updated)
}

async fn pull_model_with_progress_cli(host_root: &str, model: &str) -> std::io::Result<()> {
    use futures_util::StreamExt;
    let url = format!("{host_root}/api/pull");
    let client = reqwest::Client::new();
    let mut resp = client
        .post(&url)
        .json(&serde_json::json!({"model": model, "stream": true}))
        .send()
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(std::io::Error::other(format!(
            "failed to start pull: HTTP {}",
            resp.status()
        )));
    }

    let mut out = std::io::stderr();
    let _ = out.write_all(format!("Pulling model {model}...\n").as_bytes());
    let _ = out.flush();
    let mut buf = bytes::BytesMut::new();
    let mut totals: std::collections::HashMap<String, (u64, u64)> = Default::default();
    let mut last_completed: u64 = 0;
    let mut last_instant = std::time::Instant::now();

    let mut stream = resp.bytes_stream();
    let mut printed_header = false;
    let mut saw_success = false;
    let mut last_line_len: usize = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| std::io::Error::other(e.to_string()))?;
        buf.extend_from_slice(&chunk);
        loop {
            if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line = buf.split_to(pos + 1);
                if let Ok(text) = std::str::from_utf8(&line) {
                    let text = text.trim();
                    if text.is_empty() {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                        let status = value.get("status").and_then(|s| s.as_str()).unwrap_or("");
                        let digest = value
                            .get("digest")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string();
                        let total = value.get("total").and_then(|t| t.as_u64());
                        let completed = value.get("completed").and_then(|t| t.as_u64());
                        if let Some(t) = total {
                            let entry = totals.entry(digest.clone()).or_insert((t, 0));
                            entry.0 = t;
                        }
                        if let Some(c) = completed {
                            let entry = totals.entry(digest.clone()).or_insert((0, 0));
                            entry.1 = c;
                        }

                        let (sum_total, sum_completed) = totals
                            .values()
                            .fold((0u64, 0u64), |acc, v| (acc.0 + v.0, acc.1 + v.1));

                        if sum_total > 0 && !printed_header {
                            let gb = (sum_total as f64) / (1024.0 * 1024.0 * 1024.0);
                            let header = format!("Downloading {model}: total {gb:.2} GB\n");
                            let _ = out.write_all(header.as_bytes());
                            printed_header = true;
                        }

                        if sum_total > 0 {
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_instant).as_secs_f64().max(0.001);
                            let dbytes = sum_completed.saturating_sub(last_completed) as f64;
                            let speed_mb_s = dbytes / (1024.0 * 1024.0) / dt;
                            last_completed = sum_completed;
                            last_instant = now;
                            let done_gb = (sum_completed as f64) / (1024.0 * 1024.0 * 1024.0);
                            let total_gb = (sum_total as f64) / (1024.0 * 1024.0 * 1024.0);
                            let pct = (sum_completed as f64) * 100.0 / (sum_total as f64);
                            let line_text = format!(
                                "{done:.2}/{total:.2} GB ({pct:.1}%) {speed:.1} MB/s",
                                done = done_gb,
                                total = total_gb,
                                pct = pct,
                                speed = speed_mb_s
                            );
                            let pad = last_line_len.saturating_sub(line_text.len());
                            let line = format!(
                                "\r{text}{spaces}",
                                text = line_text,
                                spaces = " ".repeat(pad)
                            );
                            last_line_len = line_text.len();
                            let _ = out.write_all(line.as_bytes());
                            let _ = out.flush();
                        } else if !status.is_empty() {
                            let line_text = status.to_string();
                            let pad = last_line_len.saturating_sub(line_text.len());
                            let line = format!(
                                "\r{text}{spaces}",
                                text = line_text,
                                spaces = " ".repeat(pad)
                            );
                            last_line_len = line_text.len();
                            let _ = out.write_all(line.as_bytes());
                            let _ = out.flush();
                        }

                        if status == "success" {
                            let _ = out.write_all(b"\n");
                            let _ = out.flush();
                            saw_success = true;
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }
        if saw_success {
            break;
        }
    }
    if saw_success {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "model pull did not complete (no success status)",
        ))
    }
}

async fn ensure_ollama_model_available_cli(
    model: &str,
    host_root: &str,
    config_path: &std::path::Path,
) -> std::io::Result<()> {
    // 1) Always check the instance to ensure the model is actually available locally.
    //    This avoids relying solely on potentially stale entries in config.toml.
    let mut listed = read_ollama_models_list(config_path);
    let available = fetch_ollama_models(host_root).await;
    if available.iter().any(|m| m == model) {
        // Ensure the model is recorded in config.toml.
        if !listed.iter().any(|m| m == model) {
            listed.push(model.to_string());
            listed.sort();
            listed.dedup();
            let _ = save_ollama_models(config_path, &listed);
        }
        return Ok(());
    }

    // 2) Pull if allowlisted
    const ALLOWLIST: &[&str] = &["llama3.2:3b-instruct"];
    if !ALLOWLIST.iter().any(|&m| m == model) {
        return Err(std::io::Error::other(format!(
            "Model `{}` not found locally and not in allowlist for automatic download.",
            model
        )));
    }
    // Pull with progress; if the streaming connection ends before success, keep
    // waiting/polling and retry the streaming request until the model appears or succeeds.
    loop {
        match pull_model_with_progress_cli(host_root, model).await {
            Ok(()) => break,
            Err(_) => {
                let available = fetch_ollama_models(host_root).await;
                if available.iter().any(|m| m == model) {
                    break;
                }
                eprintln!("waiting for model to finish downloading...");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        }
    }
    listed.push(model.to_string());
    listed.sort();
    listed.dedup();
    let _ = save_ollama_models(config_path, &listed);
    Ok(())
}

pub async fn run_main(cli: Cli, codex_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    let Cli {
        images,
        model,
        ollama,
        config_profile,
        full_auto,
        dangerously_bypass_approvals_and_sandbox,
        cwd,
        skip_git_repo_check,
        color,
        last_message_file,
        json: json_mode,
        sandbox_mode: sandbox_mode_cli_arg,
        prompt,
        config_overrides,
    } = cli;

    // Track whether the user explicitly provided a model via --model.
    let user_specified_model = model.is_some();

    // Determine the prompt based on CLI arg and/or stdin.
    let prompt = match prompt {
        Some(p) if p != "-" => p,
        // Either `-` was passed or no positional arg.
        maybe_dash => {
            // When no arg (None) **and** stdin is a TTY, bail out early – unless the
            // user explicitly forced reading via `-`.
            let force_stdin = matches!(maybe_dash.as_deref(), Some("-"));

            if std::io::stdin().is_terminal() && !force_stdin {
                eprintln!(
                    "No prompt provided. Either specify one as an argument or pipe the prompt into stdin."
                );
                std::process::exit(1);
            }

            // Ensure the user knows we are waiting on stdin, as they may
            // have gotten into this state by mistake. If so, and they are not
            // writing to stdin, Codex will hang indefinitely, so this should
            // help them debug in that case.
            if !force_stdin {
                eprintln!("Reading prompt from stdin...");
            }
            let mut buffer = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut buffer) {
                eprintln!("Failed to read prompt from stdin: {e}");
                std::process::exit(1);
            } else if buffer.trim().is_empty() {
                eprintln!("No prompt provided via stdin.");
                std::process::exit(1);
            }
            buffer
        }
    };

    let (stdout_with_ansi, stderr_with_ansi) = match color {
        cli::Color::Always => (true, true),
        cli::Color::Never => (false, false),
        cli::Color::Auto => (
            std::io::stdout().is_terminal(),
            std::io::stderr().is_terminal(),
        ),
    };

    // TODO(mbolin): Take a more thoughtful approach to logging.
    let default_level = "error";
    let _ = tracing_subscriber::fmt()
        // Fallback to the `default_level` log filter if the environment
        // variable is not set _or_ contains an invalid value
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new(default_level))
                .unwrap_or_else(|_| EnvFilter::new(default_level)),
        )
        .with_ansi(stderr_with_ansi)
        .with_writer(std::io::stderr)
        .try_init();

    let sandbox_mode = if full_auto {
        Some(SandboxMode::WorkspaceWrite)
    } else if dangerously_bypass_approvals_and_sandbox {
        Some(SandboxMode::DangerFullAccess)
    } else {
        sandbox_mode_cli_arg.map(Into::<SandboxMode>::into)
    };

    // When the user opts into the Ollama provider via `--ollama`, ensure we
    // have a configured provider entry and that a local server is running.
    if ollama {
        if let Err(e) = codex_core::config::ensure_ollama_provider_configured_and_running().await {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }

    // Load configuration and determine approval policy
    let overrides = ConfigOverrides {
        model,
        config_profile,
        // This CLI is intended to be headless and has no affordances for asking
        // the user for approval.
        approval_policy: Some(AskForApproval::Never),
        sandbox_mode,
        cwd: cwd.map(|p| p.canonicalize().unwrap_or(p)),
        model_provider: if ollama {
            Some("ollama".to_string())
        } else {
            None
        },
        codex_linux_sandbox_exe,
        base_instructions: None,
        include_plan_tool: None,
    };
    // Parse `-c` overrides.
    let cli_kv_overrides = match config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    let config = Config::load_with_cli_overrides(cli_kv_overrides, overrides)?;

    // If the user passed both --ollama and --model, ensure the requested model
    // is present locally or pull it automatically (subject to allowlist).
    if ollama && user_specified_model {
        let model_name = config.model.clone();
        let base_url = config
            .model_provider
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        let host_root = base_url
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .to_string();
        let config_path = config.codex_home.join("config.toml");
        if let Err(e) =
            ensure_ollama_model_available_cli(&model_name, &host_root, &config_path).await
        {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    let mut event_processor: Box<dyn EventProcessor> = if json_mode {
        Box::new(EventProcessorWithJsonOutput::new(last_message_file.clone()))
    } else {
        Box::new(EventProcessorWithHumanOutput::create_with_ansi(
            stdout_with_ansi,
            &config,
            last_message_file.clone(),
        ))
    };

    // Print the effective configuration and prompt so users can see what Codex
    // is using.
    event_processor.print_config_summary(&config, &prompt);

    if !skip_git_repo_check && !is_inside_git_repo(&config) {
        eprintln!("Not inside a Git repo and --skip-git-repo-check was not specified.");
        std::process::exit(1);
    }

    let CodexConversation {
        codex: codex_wrapper,
        session_configured,
        ctrl_c,
        ..
    } = codex_wrapper::init_codex(config).await?;
    let codex = Arc::new(codex_wrapper);
    info!("Codex initialized with event: {session_configured:?}");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    {
        let codex = codex.clone();
        tokio::spawn(async move {
            loop {
                let interrupted = ctrl_c.notified();
                tokio::select! {
                    _ = interrupted => {
                        // Forward an interrupt to the codex so it can abort any in‑flight task.
                        let _ = codex
                            .submit(
                                Op::Interrupt,
                            )
                            .await;

                        // Exit the inner loop and return to the main input prompt.  The codex
                        // will emit a `TurnInterrupted` (Error) event which is drained later.
                        break;
                    }
                    res = codex.next_event() => match res {
                        Ok(event) => {
                            debug!("Received event: {event:?}");
                            if let Err(e) = tx.send(event) {
                                error!("Error sending event: {e:?}");
                                break;
                            }
                        },
                        Err(e) => {
                            error!("Error receiving event: {e:?}");
                            break;
                        }
                    }
                }
            }
        });
    }

    // Send images first, if any.
    if !images.is_empty() {
        let items: Vec<InputItem> = images
            .into_iter()
            .map(|path| InputItem::LocalImage { path })
            .collect();
        let initial_images_event_id = codex.submit(Op::UserInput { items }).await?;
        info!("Sent images with event ID: {initial_images_event_id}");
        while let Ok(event) = codex.next_event().await {
            if event.id == initial_images_event_id
                && matches!(
                    event.msg,
                    EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: _,
                    })
                )
            {
                break;
            }
        }
    }

    // Send the prompt.
    let items: Vec<InputItem> = vec![InputItem::Text { text: prompt }];
    let initial_prompt_task_id = codex.submit(Op::UserInput { items }).await?;
    info!("Sent prompt with event ID: {initial_prompt_task_id}");

    // Run the loop until the task is complete.
    while let Some(event) = rx.recv().await {
        let shutdown: CodexStatus = event_processor.process_event(event);
        match shutdown {
            CodexStatus::Running => continue,
            CodexStatus::InitiateShutdown => {
                codex.submit(Op::Shutdown).await?;
            }
            CodexStatus::Shutdown => {
                break;
            }
        }
    }

    Ok(())
}
