use crate::error::CodexErr;
use crate::error::Result as CoreResult;
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
use std::str::FromStr;
use toml_edit::DocumentMut as Document;
use toml_edit::Item;
use toml_edit::Table;
use toml_edit::Value as TomlValueEdit;

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

/// Variant that considers an explicit WireApi value; provided to centralize
/// host root computation in one place for future extension.
pub fn base_url_to_host_root_with_wire(base_url: &str, _wire_api: WireApi) -> String {
    base_url_to_host_root(base_url)
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
pub async fn probe_ollama_server(base_url: &str) -> CoreResult<bool> {
    let url = probe_url_for_base(base_url);
    let resp = reqwest::Client::new().get(url).send().await?;
    Ok(resp.status().is_success())
}
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

    // Probe reachability.
    let ok = probe_ollama_server(&base_url).await?;
    if !ok {
        return Err(CodexErr::OllamaServerUnreachable);
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
                                    for ev in pull_events_from_value(&value) { yield ev; }
                                    if let Some(err_msg) = value.get("error").and_then(|e| e.as_str()) {
                                        yield PullEvent::Status(format!("error: {err_msg}"));
                                        return;
                                    }
                                    if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
                                        if status == "success" { yield PullEvent::Success; return; }
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

/// Overwrite the recorded models list under [model_providers.ollama].models using toml_edit.
pub fn write_ollama_models_list(config_path: &Path, models: &[String]) -> io::Result<()> {
    let mut doc = read_document(config_path)?;
    {
        let tbl = upsert_provider_ollama(&mut doc);
        let mut arr = toml_edit::Array::new();
        for m in models {
            arr.push(TomlValueEdit::from(m.clone()));
        }
        tbl["models"] = Item::Value(TomlValueEdit::Array(arr));
    }
    write_document(config_path, &doc)
}

/// Write models list via a uniform name expected by higher layers.
pub fn write_config_models(config_path: &Path, models: &[String]) -> io::Result<()> {
    write_ollama_models_list(config_path, models)
}

/// Ensure `[model_providers.ollama]` exists with sensible defaults on disk.
/// Returns true if it created/updated the entry.
pub fn ensure_ollama_provider_entry(codex_home: &Path) -> io::Result<bool> {
    let config_path = codex_home.join("config.toml");
    let mut doc = read_document(&config_path)?;
    let before = doc.to_string();
    let _tbl = upsert_provider_ollama(&mut doc);
    let after = doc.to_string();
    if before != after {
        write_document(&config_path, &doc)?;
        Ok(true)
    } else {
        Ok(false)
    }
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
) -> CoreResult<()> {
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
    let _ = write_ollama_models_list(config_path, &listed);
    Ok(())
}

// ---------- toml_edit helpers ----------

fn read_document(path: &Path) -> io::Result<Document> {
    match std::fs::read_to_string(path) {
        Ok(s) => Document::from_str(&s).map_err(io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Document::new()),
        Err(e) => Err(e),
    }
}

fn write_document(path: &Path, doc: &Document) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, doc.to_string())
}

pub fn upsert_provider_ollama(doc: &mut Document) -> &mut Table {
    if doc.get("model_providers").is_none() {
        doc.as_table_mut()
            .insert("model_providers", Item::Table(Table::new()));
    } else if !doc["model_providers"].is_table() {
        doc["model_providers"] = Item::Table(Table::new());
    }

    let providers = doc["model_providers"]
        .as_table_mut()
        .expect("providers table");
    if providers.get("ollama").is_none() || !providers["ollama"].is_table() {
        providers["ollama"] = Item::Table(Table::new());
    }
    let tbl = providers["ollama"].as_table_mut().expect("ollama table");
    if !tbl.contains_key("name") {
        tbl["name"] = Item::Value(TomlValueEdit::from("Ollama"));
    }
    if !tbl.contains_key("base_url") {
        tbl["base_url"] = Item::Value(TomlValueEdit::from(DEFAULT_BASE_URL));
    }
    if !tbl.contains_key("wire_api") {
        tbl["wire_api"] = Item::Value(TomlValueEdit::from("chat"));
    }
    tbl
}

pub fn set_ollama_models(doc: &mut Document, models: &[String]) {
    let tbl = upsert_provider_ollama(doc);
    let mut arr = toml_edit::Array::new();
    for m in models {
        arr.push(TomlValueEdit::from(m.clone()));
    }
    tbl["models"] = Item::Value(TomlValueEdit::Array(arr));
}

// Convert a single JSON object representing a pull update into one or more events.
fn pull_events_from_value(value: &JsonValue) -> Vec<PullEvent> {
    let mut events = Vec::new();
    if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
        events.push(PullEvent::Status(status.to_string()));
        if status == "success" {
            events.push(PullEvent::Success);
        }
    }
    let digest = value
        .get("digest")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    let total = value.get("total").and_then(|t| t.as_u64());
    let completed = value.get("completed").and_then(|t| t.as_u64());
    if total.is_some() || completed.is_some() {
        events.push(PullEvent::ChunkProgress {
            digest,
            total,
            completed,
        });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::DocumentMut as Document;

    #[test]
    fn test_base_url_to_host_root() {
        assert_eq!(
            base_url_to_host_root("http://localhost:11434/v1"),
            "http://localhost:11434"
        );
        assert_eq!(
            base_url_to_host_root("http://localhost:11434"),
            "http://localhost:11434"
        );
        assert_eq!(
            base_url_to_host_root("http://localhost:11434/"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn test_probe_url_for_base() {
        assert_eq!(
            probe_url_for_base("http://localhost:11434/v1"),
            "http://localhost:11434/v1/models"
        );
        assert_eq!(
            probe_url_for_base("http://localhost:11434"),
            "http://localhost:11434/api/tags"
        );
    }

    #[test]
    fn test_pull_events_decoder_status_and_success() {
        let v: JsonValue = serde_json::json!({"status":"verifying"});
        let events = pull_events_from_value(&v);
        assert!(matches!(events.as_slice(), [PullEvent::Status(s)] if s == "verifying"));

        let v2: JsonValue = serde_json::json!({"status":"success"});
        let events2 = pull_events_from_value(&v2);
        assert_eq!(events2.len(), 2);
        assert!(matches!(events2[0], PullEvent::Status(ref s) if s == "success"));
        assert!(matches!(events2[1], PullEvent::Success));
    }

    #[test]
    fn test_pull_events_decoder_progress() {
        let v: JsonValue = serde_json::json!({"digest":"sha256:abc","total":100});
        let events = pull_events_from_value(&v);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PullEvent::ChunkProgress {
                digest,
                total,
                completed,
            } => {
                assert_eq!(digest, "sha256:abc");
                assert_eq!(*total, Some(100));
                assert_eq!(*completed, None);
            }
            _ => panic!("expected ChunkProgress"),
        }

        let v2: JsonValue = serde_json::json!({"digest":"sha256:def","completed":42});
        let events2 = pull_events_from_value(&v2);
        assert_eq!(events2.len(), 1);
        match &events2[0] {
            PullEvent::ChunkProgress {
                digest,
                total,
                completed,
            } => {
                assert_eq!(digest, "sha256:def");
                assert_eq!(*total, None);
                assert_eq!(*completed, Some(42));
            }
            _ => panic!("expected ChunkProgress"),
        }
    }

    #[test]
    fn test_upsert_provider_and_models() {
        let mut doc = Document::new();
        let tbl = upsert_provider_ollama(&mut doc);
        assert!(tbl.contains_key("name"));
        assert!(tbl.contains_key("base_url"));
        assert!(tbl.contains_key("wire_api"));
        set_ollama_models(&mut doc, &vec!["llama3.2:3b".to_string()]);
        let root = doc.as_table();
        let mp = root
            .get("model_providers")
            .and_then(|i| i.as_table())
            .expect("model_providers");
        let ollama = mp.get("ollama").and_then(|i| i.as_table()).expect("ollama");
        let arr = ollama.get("models").expect("models array");
        assert!(arr.is_array(), "models should be an array");
        let s = doc.to_string();
        assert!(s.contains("model_providers"));
        assert!(s.contains("ollama"));
        assert!(s.contains("models"));
    }
}
