use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use bytes::BytesMut;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::io;
use std::io::Write;
use std::path::Path;

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_WIRE_API: WireApi = WireApi::Chat;
pub const DEFAULT_PULL_ALLOWLIST: &[&str] = &["llama3.2:3b"];

/// Identify whether a base_url points at an OpenAI-compatible root (".../v1").
fn is_openai_compatible_base_url(base_url: &str) -> bool {
    base_url.trim_end_matches('/').ends_with("/v1")
}

/// Convert a provider base_url into the native Ollama host root.
/// For example, "http://localhost:11434/v1" -> "http://localhost:11434".
pub fn base_url_to_host_root(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string()
    } else {
        trimmed.to_string()
    }
}

/// Compute the probe URL to verify if an Ollama server is reachable.
/// If the configured base is OpenAI-compatible (/v1), probe "models", otherwise
/// fall back to the native "/api/tags" endpoint.
pub fn probe_url_for_base(base_url: &str) -> String {
    if is_openai_compatible_base_url(base_url) {
        format!("{}/models", base_url.trim_end_matches('/'))
    } else {
        format!("{}/api/tags", base_url.trim_end_matches('/'))
    }
}

/// Convenience helper to probe an Ollama server given a provider style base URL.
pub async fn probe_ollama_server(base_url: &str) -> io::Result<bool> {
    let host_root = base_url_to_host_root(base_url);
    let client = OllamaClient::from_host_root(host_root);
    client.probe_server().await
}
/// Coordinator wrapper used by frontends when responding to `--ollama`.
///
/// - Probes the server using the configured base_url when present, otherwise
///   falls back to DEFAULT_BASE_URL.
/// - If the server is reachable, ensures an `[model_providers.ollama]` entry
///   exists in `config.toml` with sensible defaults.
/// - If no server is reachable, returns an error.
pub async fn ensure_configured_and_running() -> io::Result<()> {
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

    // Probe reachability.
    let ok = probe_ollama_server(&base_url).await?;
    if !ok {
        return Err(io::Error::other(
            "No running Ollama server detected. Please install/start Ollama: https://github.com/ollama/ollama?tab=readme-ov-file#ollama",
        ));
    }

    // Ensure provider entry exists with defaults.
    let _ = ensure_ollama_provider_entry(&codex_home)?;
    Ok(())
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
pub struct TuiProgressReporter(CliProgressReporter);

impl Default for TuiProgressReporter {
    fn default() -> Self {
        Self(CliProgressReporter::new())
    }
}
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

/// Client for interacting with a local Ollama instance.
pub struct OllamaClient {
    client: reqwest::Client,
    host_root: String,
    uses_openai_compat: bool,
}

impl OllamaClient {
    /// Build a client from a provider definition. Falls back to the default
    /// local URL if no base_url is configured.
    pub fn from_provider(provider: &ModelProviderInfo) -> Self {
        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let uses_openai_compat = is_openai_compatible_base_url(&base_url)
            || matches!(provider.wire_api, WireApi::Chat)
                && is_openai_compatible_base_url(&base_url);
        let host_root = base_url_to_host_root(&base_url);
        Self {
            client: reqwest::Client::new(),
            host_root,
            uses_openai_compat,
        }
    }

    /// Low-level constructor given a raw host root, e.g. "http://localhost:11434".
    pub fn from_host_root(host_root: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            host_root: host_root.into(),
            uses_openai_compat: false,
        }
    }

    /// Probe whether the server is reachable by hitting the appropriate health endpoint.
    pub async fn probe_server(&self) -> io::Result<bool> {
        let url = if self.uses_openai_compat {
            format!("{}/v1/models", self.host_root.trim_end_matches('/'))
        } else {
            format!("{}/api/tags", self.host_root.trim_end_matches('/'))
        };
        let resp = self.client.get(url).send().await;
        Ok(matches!(resp, Ok(r) if r.status().is_success()))
    }

    /// Return the list of model names known to the local Ollama instance.
    pub async fn fetch_models(&self) -> io::Result<Vec<String>> {
        let tags_url = format!("{}/api/tags", self.host_root.trim_end_matches('/'));
        let resp = self
            .client
            .get(tags_url)
            .send()
            .await
            .map_err(io::Error::other)?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }
        let val = resp.json::<JsonValue>().await.map_err(io::Error::other)?;
        let names = val
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(names)
    }

    /// Start a model pull and emit streaming events. The returned stream ends when
    /// a Success event is observed or the server closes the connection.
    pub async fn pull_model_stream(
        &self,
        model: &str,
    ) -> io::Result<BoxStream<'static, PullEvent>> {
        let url = format!("{}/api/pull", self.host_root.trim_end_matches('/'));
        let resp = self
            .client
            .post(url)
            .json(&serde_json::json!({"model": model, "stream": true}))
            .send()
            .await
            .map_err(io::Error::other)?;
        if !resp.status().is_success() {
            return Err(io::Error::other(format!(
                "failed to start pull: HTTP {}",
                resp.status()
            )));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = BytesMut::new();
        let _pending: VecDeque<PullEvent> = VecDeque::new();

        // Using an async stream adaptor backed by unfold-like manual loop.
        let s = async_stream::stream! {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                            let line = buf.split_to(pos + 1);
                            if let Ok(text) = std::str::from_utf8(&line) {
                                let text = text.trim();
                                if text.is_empty() { continue; }
                                if let Ok(value) = serde_json::from_str::<JsonValue>(text) {
                                    if let Some(err_msg) = value.get("error").and_then(|e| e.as_str()) {
                                        yield PullEvent::Status(format!("error: {err_msg}"));
                                        return;
                                    }
                                    if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
                                        yield PullEvent::Status(status.to_string());
                                        if status == "success" { yield PullEvent::Success; return; }
                                    }
                                    let digest = value.get("digest").and_then(|d| d.as_str()).unwrap_or("").to_string();
                                    let total = value.get("total").and_then(|t| t.as_u64());
                                    let completed = value.get("completed").and_then(|t| t.as_u64());
                                    if total.is_some() || completed.is_some() {
                                        yield PullEvent::ChunkProgress { digest, total, completed };
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Connection error: end the stream.
                        return;
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }

    /// High-level helper to pull a model and drive a progress reporter.
    pub async fn pull_with_reporter(
        &self,
        model: &str,
        reporter: &mut dyn PullProgressReporter,
    ) -> io::Result<()> {
        reporter.on_event(&PullEvent::Status(format!("Pulling model {model}...")))?;
        let mut stream = self.pull_model_stream(model).await?;
        while let Some(event) = stream.next().await {
            reporter.on_event(&event)?;
            if matches!(event, PullEvent::Success) {
                break;
            }
        }
        Ok(())
    }
}

/// Read the list of models recorded under [model_providers.ollama].models.
pub fn read_ollama_models_list(config_path: &Path) -> Vec<String> {
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

/// Convenience wrapper that returns the models list as an io::Result for callers
/// that want a uniform Result-based API.
pub fn read_config_models(config_path: &Path) -> io::Result<Vec<String>> {
    Ok(read_ollama_models_list(config_path))
}

/// Overwrite the recorded models list under [model_providers.ollama].models.
pub fn write_ollama_models_list(config_path: &Path, models: &[String]) -> io::Result<()> {
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
        None => return Err(io::Error::other("invalid TOML root value")),
    };

    let mp_val = root_tbl
        .entry("model_providers".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    if !mp_val.is_table() {
        *mp_val = toml::Value::Table(TomlTable::new());
    }
    let mp_tbl = match mp_val.as_table_mut() {
        Some(t) => t,
        None => return Err(io::Error::other("invalid model_providers table")),
    };

    let ollama_val = mp_tbl
        .entry("ollama".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    if !ollama_val.is_table() {
        *ollama_val = toml::Value::Table(TomlTable::new());
    }
    let ollama_tbl = match ollama_val.as_table_mut() {
        Some(t) => t,
        None => return Err(io::Error::other("invalid ollama table")),
    };
    let arr = toml::Value::Array(
        models
            .iter()
            .map(|m| toml::Value::String(m.clone()))
            .collect(),
    );
    ollama_tbl.insert("models".to_string(), arr);

    let updated =
        toml::to_string_pretty(&root_value).map_err(|e| io::Error::other(e.to_string()))?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, updated)
}

/// Write models list via a uniform name expected by higher layers.
pub fn write_config_models(config_path: &Path, models: &[String]) -> io::Result<()> {
    write_ollama_models_list(config_path, models)
}

/// Ensure `[model_providers.ollama]` exists with sensible defaults on disk.
/// Returns true if it created/updated the entry.
pub fn ensure_ollama_provider_entry(codex_home: &Path) -> io::Result<bool> {
    use toml::value::Table as TomlTable;
    let config_path = codex_home.join("config.toml");
    let mut root_value = if let Ok(contents) = std::fs::read_to_string(&config_path) {
        toml::from_str::<toml::Value>(&contents).unwrap_or(toml::Value::Table(TomlTable::new()))
    } else {
        toml::Value::Table(TomlTable::new())
    };

    if !matches!(root_value, toml::Value::Table(_)) {
        root_value = toml::Value::Table(TomlTable::new());
    }
    let root_tbl = match root_value.as_table_mut() {
        Some(t) => t,
        None => return Err(io::Error::other("invalid TOML root")),
    };

    let mp_val = root_tbl
        .entry("model_providers".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    if !mp_val.is_table() {
        *mp_val = toml::Value::Table(TomlTable::new());
    }
    let mp_tbl = match mp_val.as_table_mut() {
        Some(t) => t,
        None => return Err(io::Error::other("invalid model_providers table")),
    };

    let mut changed = false;
    let ollama_val = mp_tbl.entry("ollama".to_string()).or_insert_with(|| {
        changed = true;
        toml::Value::Table(TomlTable::new())
    });
    if !ollama_val.is_table() {
        *ollama_val = toml::Value::Table(TomlTable::new());
        changed = true;
    }
    if let Some(tbl) = ollama_val.as_table_mut() {
        if !tbl.contains_key("name") {
            tbl.insert(
                "name".to_string(),
                toml::Value::String("Ollama".to_string()),
            );
            changed = true;
        }
        if !tbl.contains_key("base_url") {
            tbl.insert(
                "base_url".to_string(),
                toml::Value::String(DEFAULT_BASE_URL.to_string()),
            );
            changed = true;
        }
        if !tbl.contains_key("wire_api") {
            tbl.insert(
                "wire_api".to_string(),
                toml::Value::String("chat".to_string()),
            );
            changed = true;
        }
    }

    if changed {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let updated = toml::to_string_pretty(&root_value).map_err(io::Error::other)?;
        std::fs::write(config_path, updated)?;
    }
    Ok(changed)
}

/// Alias name mirroring the refactor plan wording.
pub fn ensure_provider_entry_and_defaults(codex_home: &Path) -> io::Result<bool> {
    ensure_ollama_provider_entry(codex_home)
}

/// Read whether the provider exists and how many models are recorded under it.
pub fn read_provider_state(config_path: &Path) -> (bool, usize) {
    match std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(toml::Value::Table(root)) => {
            let provider_present = root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("ollama"))
                .map(|_| true)
                .unwrap_or(false);
            let models_count = root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("ollama"))
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("models"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            (provider_present, models_count)
        }
        _ => (false, 0),
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
) -> io::Result<()> {
    let mut listed = read_ollama_models_list(config_path);
    let available = client.fetch_models().await.unwrap_or_default();
    if available.iter().any(|m| m == model) {
        if !listed.iter().any(|m| m == model) {
            listed.push(model.to_string());
            listed.sort();
            listed.dedup();
            let _ = write_ollama_models_list(config_path, &listed);
        }
        return Ok(());
    }

    if !DEFAULT_PULL_ALLOWLIST.contains(&model) {
        return Err(io::Error::other(format!(
            "Model `{model}` not found locally and not in allowlist for automatic download."
        )));
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
    let _ = write_ollama_models_list(config_path, &listed);
    Ok(())
}
