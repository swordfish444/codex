use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::util::append_error_log;
use chrono::Local;
use mime_guess::MimeGuess;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;
use tracing::warn;
use url::Url;

const UPLOAD_USE_CASE: &str = "codex";

/// Stable identifier assigned to each staged attachment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentId(pub u64);

impl AttachmentId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Represents the lifecycle of an attachment upload initiated after an `@` mention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentUploadState {
    NotStarted,
    Uploading(AttachmentUploadProgress),
    Uploaded(AttachmentUploadSuccess),
    Failed(AttachmentUploadError),
}

impl Default for AttachmentUploadState {
    fn default() -> Self {
        Self::NotStarted
    }
}

impl AttachmentUploadState {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::NotStarted | Self::Uploading(_))
    }

    pub fn is_uploaded(&self) -> bool {
        matches!(self, Self::Uploaded(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

/// Progress for uploads where the total size is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentUploadProgress {
    pub uploaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

impl AttachmentUploadProgress {
    pub fn new(uploaded_bytes: u64, total_bytes: Option<u64>) -> Self {
        Self {
            uploaded_bytes,
            total_bytes,
        }
    }
}

/// Successful upload metadata containing the remote pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentUploadSuccess {
    pub asset_pointer: AttachmentAssetPointer,
    pub display_name: String,
}

impl AttachmentUploadSuccess {
    pub fn new(asset_pointer: AttachmentAssetPointer, display_name: impl Into<String>) -> Self {
        Self {
            asset_pointer,
            display_name: display_name.into(),
        }
    }
}

/// Describes the remote asset pointer returned by the file service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentAssetPointer {
    pub kind: AttachmentPointerKind,
    pub value: String,
}

impl AttachmentAssetPointer {
    pub fn new(kind: AttachmentPointerKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }
}

/// High-level pointer type so we can support both single file and container uploads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentPointerKind {
    File,
    Image,
    #[allow(dead_code)]
    Container,
}

impl fmt::Display for AttachmentPointerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File => write!(f, "file"),
            Self::Image => write!(f, "image"),
            Self::Container => write!(f, "container"),
        }
    }
}

/// Captures a user-visible error when uploading an attachment fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentUploadError {
    pub message: String,
}

impl AttachmentUploadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AttachmentUploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Internal update emitted by the background uploader task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentUploadUpdate {
    Started {
        id: AttachmentId,
        total_bytes: Option<u64>,
    },
    Finished {
        id: AttachmentId,
        result: Result<AttachmentUploadSuccess, AttachmentUploadError>,
    },
}

/// Configuration for attachment uploads.
#[derive(Clone, Debug)]
pub enum AttachmentUploadMode {
    Disabled,
    #[cfg_attr(not(test), allow(dead_code))]
    ImmediateSuccess,
    Http(HttpConfig),
}

#[derive(Clone, Debug)]
pub struct HttpConfig {
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub user_agent: Option<String>,
}

impl HttpConfig {
    fn trimmed_base(&self) -> String {
        self.base_url.trim_end_matches('/').to_string()
    }
}

#[derive(Clone)]
enum AttachmentUploadBackend {
    Disabled,
    ImmediateSuccess,
    Http(Arc<AttachmentUploadHttp>),
}

#[derive(Clone)]
struct AttachmentUploadHttp {
    client: Client,
    base_url: String,
    bearer_token: Option<String>,
    chatgpt_account_id: Option<String>,
    user_agent: Option<String>,
}

impl AttachmentUploadHttp {
    fn apply_default_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut b = builder;
        if let Some(token) = &self.bearer_token {
            b = b.bearer_auth(token);
        }
        if let Some(acc) = &self.chatgpt_account_id {
            b = b.header("ChatGPT-Account-Id", acc);
        }
        if let Some(ua) = &self.user_agent {
            b = b.header(reqwest::header::USER_AGENT, ua.clone());
        }
        b
    }
}

/// Bookkeeping for in-flight attachment uploads, providing polling APIs for the UI thread.
pub struct AttachmentUploader {
    update_tx: UnboundedSender<AttachmentUploadUpdate>,
    update_rx: UnboundedReceiver<AttachmentUploadUpdate>,
    inflight: HashMap<AttachmentId, Arc<AtomicBool>>,
    backend: AttachmentUploadBackend,
}

impl AttachmentUploader {
    pub fn new(mode: AttachmentUploadMode) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let backend = match mode {
            AttachmentUploadMode::Disabled => AttachmentUploadBackend::Disabled,
            AttachmentUploadMode::ImmediateSuccess => AttachmentUploadBackend::ImmediateSuccess,
            AttachmentUploadMode::Http(cfg) => match Client::builder().build() {
                Ok(client) => AttachmentUploadBackend::Http(Arc::new(AttachmentUploadHttp {
                    client,
                    base_url: cfg.trimmed_base(),
                    bearer_token: cfg.bearer_token,
                    chatgpt_account_id: cfg.chatgpt_account_id,
                    user_agent: cfg.user_agent,
                })),
                Err(err) => {
                    warn!("attachment_upload.http_client_init_failed: {err}");
                    AttachmentUploadBackend::Disabled
                }
            },
        };
        Self {
            update_tx: tx,
            update_rx: rx,
            inflight: HashMap::new(),
            backend,
        }
    }

    pub fn start_upload(
        &mut self,
        id: AttachmentId,
        display_name: impl Into<String>,
        fs_path: PathBuf,
    ) -> Result<(), AttachmentUploadError> {
        if self.inflight.contains_key(&id) {
            return Err(AttachmentUploadError::new("upload already queued"));
        }
        if let AttachmentUploadBackend::Disabled = &self.backend {
            return Err(AttachmentUploadError::new(
                "file uploads are not available in this environment",
            ));
        }

        if !is_supported_image(&fs_path) {
            return Err(AttachmentUploadError::new(
                "only image files can be uploaded",
            ));
        }

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.inflight.insert(id, cancel_token.clone());
        let tx = self.update_tx.clone();
        let backend = self.backend.clone();
        let path_clone = fs_path.clone();
        let label = display_name.into();
        tokio::spawn(async move {
            let metadata = tokio::fs::metadata(&fs_path).await.ok();
            let total_bytes = metadata.as_ref().map(std::fs::Metadata::len);
            let _ = tx.send(AttachmentUploadUpdate::Started { id, total_bytes });

            if cancel_token.load(Ordering::Relaxed) {
                let _ = tx.send(AttachmentUploadUpdate::Finished {
                    id,
                    result: Err(AttachmentUploadError::new("upload canceled")),
                });
                return;
            }

            let result = match backend {
                AttachmentUploadBackend::Disabled => Err(AttachmentUploadError::new(
                    "file uploads are not available in this environment",
                )),
                AttachmentUploadBackend::ImmediateSuccess => {
                    let pointer = AttachmentAssetPointer::new(
                        AttachmentPointerKind::File,
                        format!("file-service://mock-{}", id.raw()),
                    );
                    Ok(AttachmentUploadSuccess::new(pointer, label.clone()))
                }
                AttachmentUploadBackend::Http(http) => {
                    perform_http_upload(
                        http,
                        &path_clone,
                        &label,
                        total_bytes,
                        cancel_token.clone(),
                    )
                    .await
                }
            };

            let _ = tx.send(AttachmentUploadUpdate::Finished { id, result });
        });
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cancel_all(&mut self) {
        for cancel in self.inflight.values() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn poll(&mut self) -> Vec<AttachmentUploadUpdate> {
        let mut out = Vec::new();
        while let Ok(update) = self.update_rx.try_recv() {
            if let AttachmentUploadUpdate::Finished { id, .. } = &update {
                self.inflight.remove(id);
            }
            out.push(update);
        }
        out
    }
}

impl Default for AttachmentUploader {
    fn default() -> Self {
        Self::new(AttachmentUploadMode::Disabled)
    }
}

async fn perform_http_upload(
    http: Arc<AttachmentUploadHttp>,
    fs_path: &Path,
    display_label: &str,
    total_bytes: Option<u64>,
    cancel_token: Arc<AtomicBool>,
) -> Result<AttachmentUploadSuccess, AttachmentUploadError> {
    let file_bytes = tokio::fs::read(fs_path)
        .await
        .map_err(|e| AttachmentUploadError::new(format!("failed to read file: {e}")))?;

    if cancel_token.load(Ordering::Relaxed) {
        return Err(AttachmentUploadError::new("upload canceled"));
    }

    let file_name = fs_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| display_label.to_string());

    let create_url = format!("{}/files", http.base_url);
    let body = CreateFileRequest {
        file_name: &file_name,
        file_size: total_bytes.unwrap_or(file_bytes.len() as u64),
        use_case: UPLOAD_USE_CASE,
        timezone_offset_min: (Local::now().offset().utc_minus_local() / 60),
        reset_rate_limits: false,
    };

    let create_resp = http
        .apply_default_headers(http.client.post(&create_url))
        .json(&body)
        .send()
        .await
        .map_err(|e| AttachmentUploadError::new(format!("file create failed: {e}")))?;
    if !create_resp.status().is_success() {
        let status = create_resp.status();
        let text = create_resp.text().await.unwrap_or_default();
        return Err(AttachmentUploadError::new(format!(
            "file create request failed status={status} body={text}"
        )));
    }
    let created: CreateFileResponse = create_resp
        .json()
        .await
        .map_err(|e| AttachmentUploadError::new(format!("decode file create response: {e}")))?;

    if cancel_token.load(Ordering::Relaxed) {
        return Err(AttachmentUploadError::new("upload canceled"));
    }

    let upload_url = resolve_upload_url(&created.upload_url)
        .ok_or_else(|| AttachmentUploadError::new("invalid upload url"))?;

    let mime = infer_image_mime(fs_path)
        .ok_or_else(|| AttachmentUploadError::new("only image files can be uploaded"))?;
    let mut azure_req = http.client.put(&upload_url);
    azure_req = azure_req
        .header("x-ms-blob-type", "BlockBlob")
        .header("x-ms-version", "2020-04-08");

    azure_req = azure_req
        .header(reqwest::header::CONTENT_TYPE, mime.as_str())
        .header("x-ms-blob-content-type", mime.as_str());

    let azure_resp = azure_req
        .body(file_bytes)
        .send()
        .await
        .map_err(|e| AttachmentUploadError::new(format!("blob upload failed: {e}")))?;

    if !(200..300).contains(&azure_resp.status().as_u16()) {
        let status = azure_resp.status();
        let text = azure_resp.text().await.unwrap_or_default();
        return Err(AttachmentUploadError::new(format!(
            "blob upload failed status={status} body={text}"
        )));
    }

    if cancel_token.load(Ordering::Relaxed) {
        return Err(AttachmentUploadError::new("upload canceled"));
    }

    // Finalization must succeed so the pointer can be used; surface any failure
    // to the caller after logging for easier debugging.
    if let Err(err) = finalize_upload(http.clone(), &created.file_id, &file_name).await {
        let reason = err.message.clone();
        warn!(
            "mention.attachment.upload.finalize_failed file_id={} reason={reason}",
            created.file_id
        );
        append_error_log(format!(
            "mention.attachment.upload.finalize_failed file_id={} reason={reason}",
            created.file_id
        ));
        return Err(err);
    }

    let pointer = asset_pointer_from_id(&created.file_id);
    debug!(
        "mention.attachment.upload.success file_id={} pointer={}",
        created.file_id, pointer
    );
    let pointer_kind = AttachmentPointerKind::Image;

    Ok(AttachmentUploadSuccess::new(
        AttachmentAssetPointer::new(pointer_kind, pointer),
        display_label,
    ))
}

fn asset_pointer_from_id(file_id: &str) -> String {
    if file_id.starts_with("file_") {
        format!("sediment://{file_id}")
    } else {
        format!("file-service://{file_id}")
    }
}

pub fn pointer_id_from_value(pointer: &str) -> Option<String> {
    pointer
        .strip_prefix("file-service://")
        .or_else(|| pointer.strip_prefix("sediment://"))
        .map(str::to_string)
        .or_else(|| (!pointer.is_empty()).then(|| pointer.to_string()))
}

async fn finalize_upload(
    http: Arc<AttachmentUploadHttp>,
    file_id: &str,
    file_name: &str,
) -> Result<(), AttachmentUploadError> {
    let finalize_url = format!("{}/files/process_upload_stream", http.base_url);
    let body = FinalizeUploadRequest {
        file_id,
        use_case: UPLOAD_USE_CASE,
        index_for_retrieval: false,
        file_name,
    };
    let finalize_resp = http
        .apply_default_headers(http.client.post(&finalize_url))
        .json(&body)
        .send()
        .await
        .map_err(|e| AttachmentUploadError::new(format!("finalize upload failed: {e}")))?;
    if !finalize_resp.status().is_success() {
        let status = finalize_resp.status();
        let text = finalize_resp.text().await.unwrap_or_default();
        return Err(AttachmentUploadError::new(format!(
            "finalize upload failed status={status} body={text}"
        )));
    }
    Ok(())
}

fn resolve_upload_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if !parsed.as_str().to_lowercase().contains("estuary") {
        return Some(parsed.into());
    }
    parsed
        .query_pairs()
        .find(|(k, _)| k == "upload_url")
        .map(|(_, v)| v.into_owned())
}

#[derive(Serialize)]
struct CreateFileRequest<'a> {
    file_name: &'a str,
    file_size: u64,
    use_case: &'a str,
    timezone_offset_min: i32,
    reset_rate_limits: bool,
}

#[derive(Serialize)]
struct FinalizeUploadRequest<'a> {
    file_id: &'a str,
    use_case: &'a str,
    index_for_retrieval: bool,
    file_name: &'a str,
}

#[derive(Deserialize)]
struct CreateFileResponse {
    file_id: String,
    upload_url: String,
}

fn is_supported_image(path: &Path) -> bool {
    infer_image_mime(path).is_some()
}

fn infer_image_mime(path: &Path) -> Option<String> {
    let guess = MimeGuess::from_path(path)
        .first_raw()
        .map(std::string::ToString::to_string);
    if let Some(m) = guess {
        if m.starts_with("image/") {
            return Some(m);
        }
    }

    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.trim().to_ascii_lowercase())?;

    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => return None,
    };

    Some(mime.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn infer_image_mime_accepts_common_extensions() {
        let cases = [
            ("foo.png", Some("image/png")),
            ("bar.JPG", Some("image/jpeg")),
            ("baz.jpeg", Some("image/jpeg")),
            ("img.gif", Some("image/gif")),
            ("slide.WEBP", Some("image/webp")),
            ("art.bmp", Some("image/bmp")),
            ("vector.svg", Some("image/svg+xml")),
            ("photo.heic", Some("image/heic")),
            ("photo.heif", Some("image/heif")),
        ];

        for (path, expected) in cases {
            let actual = infer_image_mime(Path::new(path));
            assert_eq!(actual.as_deref(), expected, "case {path}");
        }
    }

    #[test]
    fn infer_image_mime_rejects_unknown_extension() {
        assert!(infer_image_mime(Path::new("doc.txt")).is_none());
    }
}
