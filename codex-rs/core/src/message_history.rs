//! Persistence layer for the global, append-only *message history* file.
//!
//! The history is stored at `~/.codex/history.jsonl` with **one JSON object per
//! line** so that it can be efficiently appended to and parsed with standard
//! JSON-Lines tooling. Each record has the following schema:
//!
//! ````text
//! {"session_id":"<uuid>","ts":<unix_seconds>,"text":"<message>"}
//! ````
//!
//! To minimise the chance of interleaved writes when multiple processes are
//! appending concurrently, callers should *prepare the full line* (record +
//! trailing `\n`) and write it with a **single `write(2)` system call** while
//! the file descriptor is opened with the `O_APPEND` flag. POSIX guarantees
//! that writes up to `PIPE_BUF` bytes are atomic in that case.

use std::fs::OpenOptions;
use std::io::Write;

use serde::Serialize;
use uuid::Uuid;

use crate::config::Config;

/// Filename that stores the message history inside `~/.codex`.
const HISTORY_FILENAME: &str = "history.jsonl";

#[derive(Serialize)]
struct HistoryEntry<'a> {
    session_id: &'a str,
    ts: u64,
    text: &'a str,
}

/// Append a `text` entry associated with `session_id` to the history file.
///
/// This uses a *single* `write(2)` on a file opened with the `O_APPEND` flag.
/// POSIX guarantees that such writes up to `PIPE_BUF` bytes are atomic – no
/// other process can interleave its own data within the same call.  Because
/// each history record is tiny (≪ `PIPE_BUF`) we can rely on this property to
/// avoid additional synchronisation primitives or file locking.
///
/// Owing to the blocking nature of the syscall the function itself is kept
/// **synchronous**; callers running in an async context should wrap it in
/// `tokio::task::spawn_blocking` so the write does not obstruct the async
/// scheduler.
pub(crate) fn append_entry(text: &str, session_id: &Uuid, config: &Config) -> std::io::Result<()> {
    if !config.history.save {
        return Ok(());
    }

    // TODO: check `text` for sensitive patterns

    // Resolve `~/.codex/history.jsonl` and ensure the parent directory exists.
    let codex_home = config.codex_home.clone();
    std::fs::create_dir_all(&codex_home)?;
    let mut history_file = codex_home;
    history_file.push(HISTORY_FILENAME);

    // Compute timestamp (seconds since the Unix epoch).
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("system clock before Unix epoch: {e}"),
            )
        })?
        .as_secs();

    // Construct the JSON line first so we can write it in a single syscall.
    let entry = HistoryEntry {
        session_id: &session_id.to_string(),
        ts,
        text,
    };
    let mut line = serde_json::to_string(&entry).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to serialise history entry: {e}"),
        )
    })?;
    line.push('\n');

    // TODO: Consider using advisory locking (flock(2)) to prevent
    // interleaved writes from other processes.

    // Open in append-only mode so concurrent writers do not overwrite each
    // other. Using O_APPEND ensures that the kernel appends each write atomically.
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&history_file)?;

    // TODO: Enforce a maximum size for the history file.

    file.write_all(line.as_bytes())?;
    file.flush()?;
    Ok(())
}
