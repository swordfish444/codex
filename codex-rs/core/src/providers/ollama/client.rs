use bytes::BytesMut;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::Value as JsonValue;
use std::collections::VecDeque;
use std::io;

use crate::model_provider_info::{ModelProviderInfo, WireApi};

use super::parser::pull_events_from_value;
use super::url::{base_url_to_host_root, is_openai_compatible_base_url};
use super::{DEFAULT_BASE_URL, PullEvent, PullProgressReporter};

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
