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
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;

use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;
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
    let mut file = options.open(&history_file)?;

    // For files that already existed, adjust permissions if necessary.
    ensure_owner_only_permissions(&history_file)?;

    // Acquire an exclusive advisory lock with a bounded retry loop so that we
    // do not block indefinitely if another process keeps the file locked.
    acquire_exclusive_lock_with_retry(&file)?;

    // TODO: honor `config.history.max_size` and truncate the file if necessary.
    // Apparently Bash only does this check on startup, so over the course of
    // execution, it can exceed max_size. This seems like a good tradeoff, as
    // it keeps the amend logic simple.

    file.write_all(line.as_bytes())?;
    file.flush()?;

    // The lock is automatically released when `file` goes out of scope.
    Ok(())
}

/// Attempt to acquire an exclusive advisory lock on `file`, retrying up to 10
/// times (100 ms apart) if the lock is currently held by another process. This
/// prevents a potential indefinite wait while still giving other writers some
/// time to finish their operation.
fn acquire_exclusive_lock_with_retry(file: &std::fs::File) -> std::io::Result<()> {
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

/// Read the full contents of the history file and return a vector containing
/// every line (entry) as a `String`. If the history file does not exist yet,
/// an empty vector is returned.
///
/// The function acquires a shared advisory lock to avoid reading while another
/// process is writing, using the same bounded retry strategy as
/// `append_entry`.
pub(crate) fn read_history(config: &Config) -> std::io::Result<Vec<String>> {
    match config.history.persistence {
        HistoryPersistence::SaveAll => { /* proceed */ }
        HistoryPersistence::None => return Ok(Vec::new()),
    }

    let mut path = config.codex_home.clone();
    path.push(HISTORY_FILENAME);

    let file = match OpenOptions::new().read(true).open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // History file does not exist yet.
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };

    // Ensure the file has the correct permissions before reading.
    ensure_owner_only_permissions(&path)?;

    // Acquire a shared lock so that writers (who take an exclusive lock) are
    // blocked, ensuring we do not read partially-written data.
    acquire_shared_lock_with_retry(&file)?;

    let reader = BufReader::new(&file);
    let mut lines = Vec::new();
    for line_res in reader.lines() {
        lines.push(line_res?);
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------
// Random access helper
// ---------------------------------------------------------------------------

/// Given a `log_id` (on Unix this is the file's inode number) and a zero-based
/// `offset`, return the corresponding `HistoryEntry` if the identifier matches
/// the current history file **and** the requested offset exists. Any I/O or
/// parsing errors are logged and result in `None`.
#[cfg(unix)]
pub(crate) fn lookup(log_id: u64, offset: usize, config: &Config) -> Option<HistoryEntry> {
    use std::os::unix::fs::MetadataExt;

    let mut path = config.codex_home.clone();
    path.push(HISTORY_FILENAME);

    let metadata = match std::fs::metadata(&path) {
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
    if let Err(e) = ensure_owner_only_permissions(&path) {
        tracing::warn!(error = %e, "failed to set history file permissions");
        return None;
    }

    let file = match OpenOptions::new().read(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "failed to open history file");
            return None;
        }
    };

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

fn acquire_shared_lock_with_retry(file: &std::fs::File) -> std::io::Result<()> {
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
fn ensure_owner_only_permissions<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs;
        let metadata = fs::metadata(&path)?;
        let current_mode = metadata.permissions().mode() & 0o777;
        if current_mode != 0o600 {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }
    }
    // On non-Unix simply succeed.
    Ok(())
}
