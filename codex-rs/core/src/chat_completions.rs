use std::time::Duration;

use bytes::Bytes;
use eventsource_stream::Eventsource;
// (Already imported at top of file) -- no second import needed.
use futures::StreamExt;
use futures::TryStreamExt;
use reqwest::StatusCode;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

use crate::ModelProviderInfo;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::error::CodexErr;
use crate::error::Result;
use crate::flags::OPENAI_REQUEST_MAX_RETRIES;
use crate::flags::OPENAI_STREAM_IDLE_TIMEOUT_MS;
use crate::models::ContentItem;
use crate::models::ResponseItem;
use crate::util::backoff;

// -------------------------------------------------------------------------------------------------
// Optional client-side aggregation helper
// -------------------------------------------------------------------------------------------------

use futures::Stream;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Stream adapter that merges the incremental `OutputItemDone` chunks coming from
/// [`process_chat_sse`] into a *running* assistant message. Each time we receive
/// a new delta we emit a fresh [`ResponseEvent::OutputItemDone`] that contains
/// the **entire** content so far. This mirrors the on-the-fly aggregation the
/// TypeScript CLI performs.
///
/// The adapter is intentionally *lossless*: callers who do **not** opt in via
/// [`AggregateStreamExt::aggregate()`] keep receiving the original unmodified
/// events.
pub struct AggregatedChatStream<S> {
    inner: S,
    cumulative: String,
}

impl<S> Stream for AggregatedChatStream<S>
where
    S: Stream<Item = Result<ResponseEvent>> + Unpin,
{
    type Item = Result<ResponseEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(item)))) => {
                // We only aggregate assistant text messages. All other items pass through
                // unchanged.
                match &item {
                    crate::models::ResponseItem::Message { role, content }
                        if role == "assistant" =>
                    {
                        // Find the first OutputText part – Chat/Responses guarantee a single part
                        // per chunk.
                        if let Some(text) = content.iter().find_map(|c| match c {
                            crate::models::ContentItem::OutputText { text } => Some(text),
                            _ => None,
                        }) {
                            this.cumulative.push_str(text);

                            // Build a new aggregated ResponseItem with the concatenated string.
                            let agg_item = crate::models::ResponseItem::Message {
                                role: "assistant".to_string(),
                                content: vec![crate::models::ContentItem::OutputText {
                                    text: this.cumulative.clone(),
                                }],
                            };

                            return Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(agg_item))));
                        }

                        // If the message does not contain an OutputText (should never happen) just
                        // forward unchanged.
                        Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(item))))
                    }
                    _ => {
                        // Non-assistant or non-message: forward unchanged.
                        Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(item))))
                    }
                }
            }
            Poll::Ready(Some(event)) => {
                // On any other event we forward as-is. Reset cumulative buffer when the
                // stream signals completion so subsequent turns start fresh.
                if matches!(&event, Ok(ResponseEvent::Completed { .. })) {
                    this.cumulative.clear();
                }
                Poll::Ready(Some(event))
            }
            other => other,
        }
    }
}

/// Extension trait that activates aggregation on any stream of [`ResponseEvent`].
pub trait AggregateStreamExt: Stream<Item = Result<ResponseEvent>> + Sized {
    /// Returns a new stream that yields assistant messages whose `text` field
    /// is the concatenation of **all** deltas received so far.
    ///
    /// Usage:
    ///
    /// ```ignore
    /// let agg_stream = client.stream(&prompt).await?.aggregate();
    /// while let Some(event) = agg_stream.next().await {
    ///     // event now contains cumulative text
    /// }
    /// ```
    fn aggregate(self) -> AggregatedChatStream<Self> {
        AggregatedChatStream {
            inner: self,
            cumulative: String::new(),
        }
    }
}

impl<T> AggregateStreamExt for T where T: Stream<Item = Result<ResponseEvent>> + Sized {}

/// Implementation for the classic Chat Completions API. This is intentionally
/// minimal: we only stream back plain assistant text.
pub(crate) async fn stream_chat_completions(
    prompt: &Prompt,
    model: &str,
    client: &reqwest::Client,
    provider: &ModelProviderInfo,
) -> Result<ResponseStream> {
    // Build messages array
    let mut messages = Vec::<serde_json::Value>::new();

    if let Some(instr) = &prompt.instructions {
        messages.push(json!({"role": "system", "content": instr}));
    }

    for item in &prompt.input {
        if let ResponseItem::Message { role, content } = item {
            let mut text = String::new();
            for c in content {
                match c {
                    ContentItem::InputText { text: t } | ContentItem::OutputText { text: t } => {
                        text.push_str(t);
                    }
                    _ => {}
                }
            }
            messages.push(json!({"role": role, "content": text}));
        }
    }

    let payload = json!({
        "model": model,
        "messages": messages,
        "stream": true
    });

    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    debug!(url, "POST (chat)");
    trace!("request payload: {}", payload);

    let api_key = provider.api_key()?;
    let mut attempt = 0;
    loop {
        attempt += 1;

        let mut req_builder = client.post(&url);
        if let Some(api_key) = &api_key {
            req_builder = req_builder.bearer_auth(api_key.clone());
        }
        let res = req_builder
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&payload)
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => {
                let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(16);
                let stream = resp.bytes_stream().map_err(CodexErr::Reqwest);
                tokio::spawn(process_chat_sse(stream, tx_event));
                return Ok(ResponseStream { rx_event });
            }
            Ok(res) => {
                let status = res.status();
                if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) {
                    let body = (res.text().await).unwrap_or_default();
                    return Err(CodexErr::UnexpectedStatus(status, body));
                }

                if attempt > *OPENAI_REQUEST_MAX_RETRIES {
                    return Err(CodexErr::RetryLimit(status));
                }

                let retry_after_secs = res
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                let delay = retry_after_secs
                    .map(|s| Duration::from_millis(s * 1_000))
                    .unwrap_or_else(|| backoff(attempt));
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                if attempt > *OPENAI_REQUEST_MAX_RETRIES {
                    return Err(e.into());
                }
                let delay = backoff(attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Lightweight SSE processor for the Chat Completions streaming format. The
/// output is mapped onto Codex's internal [`ResponseEvent`] so that the rest
/// of the pipeline can stay agnostic of the underlying wire format.
async fn process_chat_sse<S>(stream: S, tx_event: mpsc::Sender<Result<ResponseEvent>>)
where
    S: Stream<Item = Result<Bytes>> + Unpin,
{
    let mut stream = stream.eventsource();

    let idle_timeout = *OPENAI_STREAM_IDLE_TIMEOUT_MS;

    loop {
        let sse = match timeout(idle_timeout, stream.next()).await {
            Ok(Some(Ok(ev))) => ev,
            Ok(Some(Err(e))) => {
                let _ = tx_event.send(Err(CodexErr::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                // Stream closed gracefully – emit Completed with dummy id.
                let _ = tx_event
                    .send(Ok(ResponseEvent::Completed {
                        response_id: String::new(),
                    }))
                    .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        // OpenAI Chat streaming sends a literal string "[DONE]" when finished.
        if sse.data.trim() == "[DONE]" {
            let _ = tx_event
                .send(Ok(ResponseEvent::Completed {
                    response_id: String::new(),
                }))
                .await;
            return;
        }

        // Parse JSON chunk
        let chunk: serde_json::Value = match serde_json::from_str(&sse.data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let content_opt = chunk
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str());

        if let Some(content) = content_opt {
            let item = ResponseItem::Message {
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: content.to_string(),
                }],
            };

            let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
        }
    }
}
