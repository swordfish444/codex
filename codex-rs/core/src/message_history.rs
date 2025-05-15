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

use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Result;
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::config::Config;
use crate::config::HistoryPersistence;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Filename that stores the message history inside `~/.codex`.
const HISTORY_FILENAME: &str = "history.jsonl";

const MAX_RETRIES: usize = 10;
const RETRY_SLEEP: Duration = Duration::from_millis(100);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryEntry {
    pub session_id: String,
    pub ts: u64,
    pub text: String,
}

fn history_filepath(config: &Config) -> PathBuf {
    let mut path = config.codex_home.clone();
    path.push(HISTORY_FILENAME);
    path
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
pub(crate) fn append_entry(text: &str, session_id: &Uuid, config: &Config) -> Result<()> {
    match config.history.persistence {
        HistoryPersistence::SaveAll => {
            // Save everything: proceed.
        }
        HistoryPersistence::None => {
            // No history persistence requested.
            return Ok(());
        }
    }

    // TODO: check `text` for sensitive patterns

    // Resolve `~/.codex/history.jsonl` and ensure the parent directory exists.
    let path = history_filepath(config);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

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
        session_id: session_id.to_string(),
        ts,
        text: text.to_string(),
    };
    let mut line = serde_json::to_string(&entry).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to serialise history entry: {e}"),
        )
    })?;
    line.push('\n');

    // Open in append-only mode so concurrent writers do not overwrite each
    // other. Using O_APPEND ensures that the kernel appends each write atomically.
    // We also open the file for reading so that `fs2` locking works on all
    // platforms.
    let mut options = OpenOptions::new();
    options.append(true).read(true).create(true);
    #[cfg(unix)]
    {
        // Ensure file is created with permissions 0o600.
        options.mode(0o600);
    }
    let mut history_file = options.open(&path)?;

    // For files that already existed, adjust permissions if necessary.
    ensure_owner_only_permissions(&history_file)?;

    // Acquire an exclusive advisory lock with a bounded retry loop so that we
    // do not block indefinitely if another process keeps the file locked.
    acquire_exclusive_lock_with_retry(&history_file)?;

    // TODO: honor `config.history.max_size` and truncate the file if necessary.
    // Apparently Bash only does this check on startup, so over the course of
    // execution, it can exceed max_size. This seems like a good tradeoff, as
    // it keeps the amend logic simple.

    history_file.write_all(line.as_bytes())?;
    history_file.flush()?;

    // The lock is automatically released when `file` goes out of scope.
    Ok(())
}

/// Attempt to acquire an exclusive advisory lock on `file`, retrying up to 10
/// times (100 ms apart) if the lock is currently held by another process. This
/// prevents a potential indefinite wait while still giving other writers some
/// time to finish their operation.
fn acquire_exclusive_lock_with_retry(file: &std::fs::File) -> Result<()> {
    for _ in 0..MAX_RETRIES {
        match fs2::FileExt::try_lock_exclusive(file) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(RETRY_SLEEP);
            }
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "could not acquire exclusive lock on history file after multiple attempts",
    ))
}

/// Asynchronously fetch the history file's *identifier* (inode on Unix) and
/// the current number of entries by counting newline characters.  This avoids
/// allocating a `String` per line and runs the blocking work in a dedicated
/// thread so it does not obstruct the async runtime.
pub(crate) async fn history_metadata(config: &Config) -> (u64, usize) {
    let path = history_filepath(config);

    // Obtain metadata (async) to get the identifier.
    let meta = match fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (0, 0),
        Err(_) => return (0, 0),
    };

    #[cfg(unix)]
    let log_id = {
        use std::os::unix::fs::MetadataExt;
        meta.ino()
    };
    #[cfg(not(unix))]
    let log_id = 0u64;

    // Open the file.
    let mut file = match fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => return (log_id, 0),
    };

    // Count newline bytes.
    let mut buf = [0u8; 8192];
    let mut count = 0usize;
    loop {
        match file.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                count += buf[..n].iter().filter(|&&b| b == b'\n').count();
            }
            Err(_) => return (log_id, 0),
        }
    }

    (log_id, count)
}

/// Given a `log_id` (on Unix this is the file's inode number) and a zero-based
/// `offset`, return the corresponding `HistoryEntry` if the identifier matches
/// the current history file **and** the requested offset exists. Any I/O or
/// parsing errors are logged and result in `None`.
#[cfg(unix)]
pub(crate) fn lookup(log_id: u64, offset: usize, config: &Config) -> Option<HistoryEntry> {
    use std::os::unix::fs::MetadataExt;

    let path = history_filepath(config);
    let file: File = match OpenOptions::new().read(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "failed to open history file");
            return None;
        }
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "failed to stat history file");
            return None;
        }
    };

    if metadata.ino() != log_id {
        return None;
    }

    // Open & lock file for reading.
    if let Err(e) = acquire_shared_lock_with_retry(&file) {
        tracing::warn!(error = %e, "failed to acquire shared lock on history file");
        return None;
    }

    let reader = BufReader::new(&file);
    for (idx, line_res) in reader.lines().enumerate() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read line from history file");
                return None;
            }
        };

        if idx == offset {
            match serde_json::from_str::<HistoryEntry>(&line) {
                Ok(entry) => return Some(entry),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse history entry");
                    return None;
                }
            }
        }
    }

    None
}

/// Fallback stub for non-Unix systems: currently always returns `None`.
#[cfg(not(unix))]
pub(crate) fn lookup(log_id: u64, offset: usize, config: &Config) -> Option<HistoryEntry> {
    let _ = (log_id, offset, config);
    None
}

fn acquire_shared_lock_with_retry(file: &File) -> Result<()> {
    for _ in 0..MAX_RETRIES {
        match fs2::FileExt::try_lock_shared(file) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(RETRY_SLEEP);
            }
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "could not acquire shared lock on history file after multiple attempts",
    ))
}

/// On Unix systems ensure the file permissions are `0o600` (rw-------). On
/// non-Unix platforms this function is a no-op. If the permissions cannot be
/// changed the error is propagated to the caller.
fn ensure_owner_only_permissions(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        let metadata = file.metadata()?;
        let current_mode = metadata.permissions().mode() & 0o777;
        if current_mode != 0o600 {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            file.set_permissions(perms)?;
        }
    }
    // On non-Unix simply succeed.
    Ok(())
}
