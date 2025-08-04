use crate::error::CodexErr;
use crate::error::Result as CoreResult;
use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::path::Path;

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_WIRE_API: crate::model_provider_info::WireApi =
    crate::model_provider_info::WireApi::Chat;
pub const DEFAULT_PULL_ALLOWLIST: &[&str] = &["llama3.2:3b"];

pub mod client;
pub mod config;
pub mod parser;
pub mod url;

pub use client::OllamaClient;
pub use config::read_config_models;
pub use config::read_provider_state;
pub use config::write_config_models;
pub use url::base_url_to_host_root;
pub use url::base_url_to_host_root_with_wire;
pub use url::probe_ollama_server;
pub use url::probe_url_for_base;
/// Coordinator wrapper used by frontends when responding to `--ollama`.
///
/// - Probes the server using the configured base_url when present, otherwise
///   falls back to DEFAULT_BASE_URL.
/// - If the server is reachable, ensures an `[model_providers.ollama]` entry
///   exists in `config.toml` with sensible defaults.
/// - If no server is reachable, returns an error.
pub async fn ensure_configured_and_running() -> CoreResult<()> {
    use crate::config::find_codex_home;
    use toml::Value as TomlValue;

    let codex_home = find_codex_home()?;
    let config_path = codex_home.join("config.toml");
    // Try to read a configured base_url if present.
    let base_url = match std::fs::read_to_string(&config_path) {
        Ok(contents) => match toml::from_str::<TomlValue>(&contents) {
            Ok(TomlValue::Table(root)) => root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("ollama"))
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("base_url"))
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_BASE_URL)
                .to_string(),
            _ => DEFAULT_BASE_URL.to_string(),
        },
        Err(_) => DEFAULT_BASE_URL.to_string(),
    };

    // Probe reachability; map any probe error to a friendly unreachable message.
    let ok: bool = url::probe_ollama_server(&base_url)
        .await
        .unwrap_or_default();
    if !ok {
        return Err(CodexErr::OllamaServerUnreachable);
    }

    // Ensure provider entry exists with defaults.
    let _ = config::ensure_ollama_provider_entry(&codex_home)?;
    Ok(())
}

#[cfg(test)]
mod ensure_tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn test_ensure_configured_returns_friendly_error_when_unreachable() {
        // Skip in CI sandbox environments without network to avoid false negatives.
        if std::env::var(crate::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
            tracing::info!(
                "{} is set; skipping test_ensure_configured_returns_friendly_error_when_unreachable",
                crate::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR
            );
            return;
        }

        let tmpdir = tempfile::TempDir::new().expect("tempdir");
        let config_path = tmpdir.path().join("config.toml");
        std::fs::create_dir_all(tmpdir.path()).unwrap();
        std::fs::write(
            &config_path,
            r#"[model_providers.ollama]
name = "Ollama"
base_url = "http://127.0.0.1:1/v1"
wire_api = "chat"
"#,
        )
        .unwrap();
        unsafe {
            std::env::set_var("CODEX_HOME", tmpdir.path());
        }

        let err = ensure_configured_and_running()
            .await
            .expect_err("should report unreachable server as friendly error");
        assert!(matches!(err, CodexErr::OllamaServerUnreachable));
    }
}

/// Events emitted while pulling a model from Ollama.
#[derive(Debug, Clone)]
pub enum PullEvent {
    /// A human-readable status message (e.g., "verifying", "writing").
    Status(String),
    /// Byte-level progress update for a specific layer digest.
    ChunkProgress {
        digest: String,
        total: Option<u64>,
        completed: Option<u64>,
    },
    /// The pull finished successfully.
    Success,
}

/// A simple observer for pull progress events. Implementations decide how to
/// render progress (CLI, TUI, logs, ...).
pub trait PullProgressReporter {
    fn on_event(&mut self, event: &PullEvent) -> io::Result<()>;
}

/// A minimal CLI reporter that writes inline progress to stderr.
pub struct CliProgressReporter {
    printed_header: bool,
    last_line_len: usize,
    last_completed_sum: u64,
    last_instant: std::time::Instant,
    totals_by_digest: HashMap<String, (u64, u64)>,
}

impl Default for CliProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl CliProgressReporter {
    pub fn new() -> Self {
        Self {
            printed_header: false,
            last_line_len: 0,
            last_completed_sum: 0,
            last_instant: std::time::Instant::now(),
            totals_by_digest: HashMap::new(),
        }
    }
}

impl PullProgressReporter for CliProgressReporter {
    fn on_event(&mut self, event: &PullEvent) -> io::Result<()> {
        let mut out = std::io::stderr();
        match event {
            PullEvent::Status(status) => {
                // Avoid noisy manifest messages; otherwise show status inline.
                if status.eq_ignore_ascii_case("pulling manifest") {
                    return Ok(());
                }
                let pad = self.last_line_len.saturating_sub(status.len());
                let line = format!("\r{status}{}", " ".repeat(pad));
                self.last_line_len = status.len();
                out.write_all(line.as_bytes())?;
                out.flush()
            }
            PullEvent::ChunkProgress {
                digest,
                total,
                completed,
            } => {
                if let Some(t) = *total {
                    self.totals_by_digest
                        .entry(digest.clone())
                        .or_insert((0, 0))
                        .0 = t;
                }
                if let Some(c) = *completed {
                    self.totals_by_digest
                        .entry(digest.clone())
                        .or_insert((0, 0))
                        .1 = c;
                }

                let (sum_total, sum_completed) = self
                    .totals_by_digest
                    .values()
                    .fold((0u64, 0u64), |acc, (t, c)| (acc.0 + *t, acc.1 + *c));
                if sum_total > 0 {
                    if !self.printed_header {
                        let gb = (sum_total as f64) / (1024.0 * 1024.0 * 1024.0);
                        let header = format!("Downloading model: total {gb:.2} GB\n");
                        out.write_all(b"\r\x1b[2K")?;
                        out.write_all(header.as_bytes())?;
                        self.printed_header = true;
                    }
                    let now = std::time::Instant::now();
                    let dt = now
                        .duration_since(self.last_instant)
                        .as_secs_f64()
                        .max(0.001);
                    let dbytes = sum_completed.saturating_sub(self.last_completed_sum) as f64;
                    let speed_mb_s = dbytes / (1024.0 * 1024.0) / dt;
                    self.last_completed_sum = sum_completed;
                    self.last_instant = now;

                    let done_gb = (sum_completed as f64) / (1024.0 * 1024.0 * 1024.0);
                    let total_gb = (sum_total as f64) / (1024.0 * 1024.0 * 1024.0);
                    let pct = (sum_completed as f64) * 100.0 / (sum_total as f64);
                    let text =
                        format!("{done_gb:.2}/{total_gb:.2} GB ({pct:.1}%) {speed_mb_s:.1} MB/s");
                    let pad = self.last_line_len.saturating_sub(text.len());
                    let line = format!("\r{text}{}", " ".repeat(pad));
                    self.last_line_len = text.len();
                    out.write_all(line.as_bytes())?;
                    out.flush()
                } else {
                    Ok(())
                }
            }
            PullEvent::Success => {
                out.write_all(b"\n")?;
                out.flush()
            }
        }
    }
}

/// For now the TUI reporter delegates to the CLI reporter. This keeps UI and
/// CLI behavior aligned until a dedicated TUI integration is implemented.
#[derive(Default)]
pub struct TuiProgressReporter(CliProgressReporter);
impl TuiProgressReporter {
    pub fn new() -> Self {
        Default::default()
    }
}
impl PullProgressReporter for TuiProgressReporter {
    fn on_event(&mut self, event: &PullEvent) -> io::Result<()> {
        self.0.on_event(event)
    }
}
/// Ensure a model is available locally.
///
/// - If the model is already present, ensure it is recorded in config.toml.
/// - If missing and in the default allowlist, pull it with streaming progress
///   and record it in config.toml after success.
/// - If missing and not allowlisted, return an error.
pub async fn ensure_model_available(
    model: &str,
    client: &OllamaClient,
    config_path: &Path,
    reporter: &mut dyn PullProgressReporter,
) -> CoreResult<()> {
    let mut listed = config::read_ollama_models_list(config_path);
    let available = client.fetch_models().await.unwrap_or_default();
    if available.iter().any(|m| m == model) {
        if !listed.iter().any(|m| m == model) {
            listed.push(model.to_string());
            listed.sort();
            listed.dedup();
            let _ = config::write_ollama_models_list(config_path, &listed);
        }
        return Ok(());
    }

    if !DEFAULT_PULL_ALLOWLIST.contains(&model) {
        return Err(CodexErr::OllamaModelNotFound(model.to_string()));
    }

    loop {
        let _ = client.pull_with_reporter(model, reporter).await;
        // After the stream completes (success or early EOF), check again.
        let available = client.fetch_models().await.unwrap_or_default();
        if available.iter().any(|m| m == model) {
            break;
        }
        // Keep waiting for the model to finish downloading.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    listed.push(model.to_string());
    listed.sort();
    listed.dedup();
    let _ = config::write_ollama_models_list(config_path, &listed);
    Ok(())
}
