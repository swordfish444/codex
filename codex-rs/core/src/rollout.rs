//! Functionality to persist a Codex conversation *rollout* – a linear list of
//! [`ResponseItem`] objects exchanged during a session – to disk so that
//! sessions can be replayed or inspected later (mirrors the behaviour of the
//! upstream TypeScript implementation).

use std::fs::File;
use std::fs::{self};
use std::io::Write;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::OffsetDateTime;

use serde::Serialize;
use uuid::Uuid;

use crate::config::codex_dir;
use crate::models::ResponseItem;

/// Folder inside `~/.codex` that holds saved rollouts.
const SESSIONS_SUBDIR: &str = "sessions";

#[derive(Serialize)]
struct SessionMeta {
    id: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
}

/// Records all [`ResponseItem`]s for a session and flushes them to disk after
/// every update.
pub(crate) struct RolloutRecorder {
    file: File,
}

impl RolloutRecorder {
    /// Attempt to create a new [`RolloutRecorder`]. If the sessions directory
    /// cannot be created or the rollout file cannot be opened we return the
    /// error so the caller can decide whether to disable persistence.
    pub fn new(instructions: Option<String>) -> std::io::Result<Self> {
        let LogFileInfo {
            file,
            session_id,
            timestamp,
        } = create_log_file()?;

        // Build the static session metadata JSON first.
        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = timestamp.format(timestamp_format).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to format timestamp: {e}"),
            )
        })?;

        let meta = SessionMeta {
            timestamp,
            id: session_id.to_string(),
            instructions,
        };

        let mut recorder = Self { file };
        recorder.record_item(&meta)?;

        Ok(recorder)
    }

    pub(crate) fn record_items(&mut self, items: &[ResponseItem]) -> std::io::Result<()> {
        for item in items {
            self.record_item(item)?;
        }
        Ok(())
    }

    fn record_item(&mut self, item: &impl Serialize) -> std::io::Result<()> {
        // Serialize the items to JSON and write them to the file.
        let json = serde_json::to_string(item).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to serialize response items: {e}"),
            )
        })?;
        writeln!(self.file, "{json}")?;
        self.file.flush()?;

        Ok(())
    }
}

struct LogFileInfo {
    /// Opened file handle to the rollout file.
    file: File,

    /// Session ID (also embedded in filename).
    session_id: Uuid,

    timestamp: OffsetDateTime,
}

fn create_log_file() -> std::io::Result<LogFileInfo> {
    // Resolve ~/.codex/sessions and create it if missing.
    let mut dir = codex_dir()?;
    dir.push(SESSIONS_SUBDIR);
    fs::create_dir_all(&dir)?;

    // Generate a v4 UUID – matches the JS CLI implementation.
    let session_id = Uuid::new_v4();
    let timestamp = OffsetDateTime::now_utc();
    // Custom format for YYYY-MM-DD
    let format: &[FormatItem] = format_description!("[year]-[month]-[day]");
    let date_str = timestamp.format(format).unwrap();

    let filename = format!("rollout-{date_str}-{session_id}.jsonl");

    let path = dir.join(filename);
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;

    Ok(LogFileInfo {
        file,
        session_id,
        timestamp,
    })
}
