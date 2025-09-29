use std::fs;
use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use codex_protocol::mcp_protocol::ConversationId;
use codex_protocol::protocol::GitInfo;
use codex_protocol::protocol::InitialHistory;
use codex_protocol::protocol::ResumedHistory;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::RolloutLine;
use codex_protocol::protocol::SessionMeta;
use codex_protocol::protocol::SessionMetaLine;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::info;
use tracing::warn;

use super::SESSIONS_SUBDIR;
use super::list::ConversationsPage;
use super::list::Cursor;
use super::list::get_conversations;
use super::policy::is_persisted_response_item;

#[async_trait]
pub trait GitInfoCollector: Send + Sync {
    async fn collect(&self, cwd: &Path) -> Option<GitInfo>;
}

#[derive(Clone)]
pub struct RolloutConfig {
    pub codex_home: PathBuf,
    pub originator: String,
    pub cli_version: String,
    pub git_info_collector: Option<Arc<dyn GitInfoCollector>>,
}

#[derive(Clone)]
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,
    rollout_path: PathBuf,
}

#[derive(Clone)]
pub enum RolloutRecorderParams {
    Create {
        conversation_id: ConversationId,
        cwd: PathBuf,
        instructions: Option<String>,
    },
    Resume {
        path: PathBuf,
    },
}

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    Flush { ack: oneshot::Sender<()> },
    Shutdown { ack: oneshot::Sender<()> },
}

impl RolloutRecorderParams {
    pub fn new(
        conversation_id: ConversationId,
        cwd: PathBuf,
        instructions: Option<String>,
    ) -> Self {
        Self::Create {
            conversation_id,
            cwd,
            instructions,
        }
    }

    pub fn resume(path: PathBuf) -> Self {
        Self::Resume { path }
    }
}

impl RolloutRecorder {
    pub async fn list_conversations(
        codex_home: &Path,
        page_size: usize,
        cursor: Option<&Cursor>,
    ) -> std::io::Result<ConversationsPage> {
        get_conversations(codex_home, page_size, cursor).await
    }

    pub async fn new(
        config: &RolloutConfig,
        params: RolloutRecorderParams,
    ) -> std::io::Result<Self> {
        let (file, rollout_path, meta, cwd) = match params {
            RolloutRecorderParams::Create {
                conversation_id,
                cwd,
                instructions,
            } => {
                let LogFileInfo {
                    file,
                    path,
                    conversation_id: session_id,
                    timestamp,
                } = create_log_file(&config.codex_home, conversation_id)?;

                let timestamp_format: &[FormatItem] = format_description!(
                    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
                );
                let timestamp = timestamp
                    .to_offset(time::UtcOffset::UTC)
                    .format(timestamp_format)
                    .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

                let meta = SessionMeta {
                    id: session_id,
                    timestamp,
                    cwd: cwd.clone(),
                    originator: config.originator.clone(),
                    cli_version: config.cli_version.clone(),
                    instructions,
                };

                (tokio::fs::File::from_std(file), path, Some(meta), Some(cwd))
            }
            RolloutRecorderParams::Resume { path } => (
                tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .await?,
                path,
                None,
                None,
            ),
        };

        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);
        let collector = config.git_info_collector.clone();

        tokio::task::spawn(rollout_writer(file, rx, meta, cwd, collector));

        Ok(Self { tx, rollout_path })
    }

    pub async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()> {
        let mut filtered = Vec::new();
        for item in items {
            if is_persisted_response_item(item) {
                filtered.push(item.clone());
            }
        }
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout items: {e}")))
    }

    pub async fn flush(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::Flush { ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout flush: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("failed waiting for rollout flush: {e}")))
    }

    pub async fn shutdown(&self) -> std::io::Result<()> {
        let (tx_done, rx_done) = oneshot::channel();
        match self.tx.send(RolloutCmd::Shutdown { ack: tx_done }).await {
            Ok(_) => rx_done
                .await
                .map_err(|e| IoError::other(format!("failed waiting for rollout shutdown: {e}"))),
            Err(e) => {
                warn!("failed to send rollout shutdown command: {e}");
                Err(IoError::other(format!(
                    "failed to send rollout shutdown command: {e}"
                )))
            }
        }
    }

    pub fn get_rollout_path(&self) -> PathBuf {
        self.rollout_path.clone()
    }

    pub async fn get_rollout_history(path: &Path) -> std::io::Result<InitialHistory> {
        info!("Resuming rollout from {path:?}");
        let text = tokio::fs::read_to_string(path).await?;
        if text.trim().is_empty() {
            return Err(IoError::other("empty session file"));
        }

        let mut items: Vec<RolloutItem> = Vec::new();
        let mut conversation_id: Option<ConversationId> = None;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    warn!("failed to parse line as JSON: {line:?}, error: {e}");
                    continue;
                }
            };

            match serde_json::from_value::<RolloutLine>(v.clone()) {
                Ok(rollout_line) => match rollout_line.item {
                    RolloutItem::SessionMeta(session_meta_line) => {
                        if conversation_id.is_none() {
                            conversation_id = Some(session_meta_line.meta.id);
                        }
                        items.push(RolloutItem::SessionMeta(session_meta_line));
                    }
                    other => items.push(other),
                },
                Err(e) => warn!("failed to parse rollout line: {v:?}, error: {e}"),
            }
        }

        info!(
            "Resumed rollout with {} items, conversation ID: {:?}",
            items.len(),
            conversation_id
        );
        let conversation_id = conversation_id
            .ok_or_else(|| IoError::other("failed to parse conversation ID from rollout file"))?;

        if items.is_empty() {
            return Ok(InitialHistory::New);
        }

        info!("Resumed rollout successfully from {path:?}");
        Ok(InitialHistory::Resumed(ResumedHistory {
            conversation_id,
            history: items,
            rollout_path: path.to_path_buf(),
        }))
    }
}

struct LogFileInfo {
    file: File,
    path: PathBuf,
    conversation_id: ConversationId,
    timestamp: OffsetDateTime,
}

fn create_log_file(
    codex_home: &Path,
    conversation_id: ConversationId,
) -> std::io::Result<LogFileInfo> {
    let timestamp = OffsetDateTime::now_local()
        .map_err(|e| IoError::other(format!("failed to get local time: {e}")))?;
    let mut dir = codex_home.to_path_buf();
    dir.push(SESSIONS_SUBDIR);
    dir.push(timestamp.year().to_string());
    dir.push(format!("{:02}", u8::from(timestamp.month())));
    dir.push(format!("{:02}", timestamp.day()));
    fs::create_dir_all(&dir)?;

    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let date_str = timestamp
        .format(format)
        .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

    let filename = format!("rollout-{date_str}-{conversation_id}.jsonl");
    let path = dir.join(filename);
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;

    Ok(LogFileInfo {
        file,
        path,
        conversation_id,
        timestamp,
    })
}

async fn rollout_writer(
    file: tokio::fs::File,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    cwd: Option<PathBuf>,
    git_info_collector: Option<Arc<dyn GitInfoCollector>>,
) -> std::io::Result<()> {
    let mut writer = JsonlWriter { file };

    if let Some(session_meta) = meta.take() {
        let git_info =
            if let (Some(provider), Some(cwd)) = (git_info_collector.as_ref(), cwd.as_ref()) {
                provider.collect(cwd.as_path()).await
            } else {
                None
            };
        let session_meta_line = SessionMetaLine {
            meta: session_meta,
            git: git_info,
        };
        writer
            .write_rollout_item(RolloutItem::SessionMeta(session_meta_line))
            .await?;
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                for item in items {
                    if is_persisted_response_item(&item) {
                        writer.write_rollout_item(item).await?;
                    }
                }
            }
            RolloutCmd::Flush { ack } => {
                if let Err(e) = writer.file.flush().await {
                    let _ = ack.send(());
                    return Err(e);
                }
                let _ = ack.send(());
            }
            RolloutCmd::Shutdown { ack } => {
                let _ = ack.send(());
            }
        }
    }

    Ok(())
}

struct JsonlWriter {
    file: tokio::fs::File,
}

impl JsonlWriter {
    async fn write_rollout_item(&mut self, rollout_item: RolloutItem) -> std::io::Result<()> {
        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = OffsetDateTime::now_utc()
            .format(timestamp_format)
            .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

        let line = RolloutLine {
            timestamp,
            item: rollout_item,
        };
        self.write_line(&line).await
    }

    async fn write_line(&mut self, item: &impl serde::Serialize) -> std::io::Result<()> {
        let mut buf = serde_json::to_vec(item)
            .map_err(|e| IoError::other(format!("failed to serialise rollout line: {e}")))?;
        buf.push(b'\n');
        self.file
            .write_all(&buf)
            .await
            .map_err(|e| IoError::other(format!("failed to write rollout line: {e}")))
    }
}
