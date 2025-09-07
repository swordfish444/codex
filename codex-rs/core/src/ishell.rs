use portable_pty::CommandBuilder;
use portable_pty::PtySize;
use portable_pty::native_pty_system;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::env;
use std::io::ErrorKind;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::Instant;

use crate::exec_command::ExecCommandSession;

const DEFAULT_TIMEOUT_MS: u64 = 250;
// Cap on how many bytes of interactive shell output we retain per request.
// If exceeded, output is truncated in the middle, preserving the beginning and end
// with an elision marker to indicate how much was removed.
const ISHELL_OUTPUT_MAX_BYTES: usize = 16 * 1024; // 16 KiB

#[derive(Debug)]
pub(crate) struct InteractiveShellRequest<'a> {
    pub session_id: Option<i32>,
    pub input_chunks: &'a [String],
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InteractiveShellResult {
    pub session_id: Option<i32>,
    pub output: String,
}

#[derive(Debug, Default)]
pub(crate) struct InteractiveShellSessionManager {
    next_session_id: AtomicI32,
    sessions: Mutex<HashMap<i32, ManagedInteractiveSession>>,
}

#[derive(Debug)]
struct ManagedInteractiveSession {
    session: ExecCommandSession,
    output_buffer: OutputBuffer,
    output_notify: Arc<Notify>,
    output_task: JoinHandle<()>,
}

type OutputBuffer = Arc<Mutex<VecDeque<Vec<u8>>>>;
type OutputHandles = (OutputBuffer, Arc<Notify>);

impl ManagedInteractiveSession {
    fn new(session: ExecCommandSession) -> Self {
        let output_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let output_notify = Arc::new(Notify::new());
        let mut receiver = session.output_receiver();
        let buffer_clone = Arc::clone(&output_buffer);
        let notify_clone = Arc::clone(&output_notify);
        let output_task = tokio::spawn(async move {
            while let Ok(chunk) = receiver.recv().await {
                let mut guard = buffer_clone.lock().await;
                guard.push_back(chunk);
                drop(guard);
                notify_clone.notify_waiters();
            }
        });

        Self {
            session,
            output_buffer,
            output_notify,
            output_task,
        }
    }

    fn writer_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.session.writer_sender()
    }

    fn output_handles(&self) -> OutputHandles {
        (
            Arc::clone(&self.output_buffer),
            Arc::clone(&self.output_notify),
        )
    }

    fn has_exited(&self) -> bool {
        self.session.has_exited()
    }
}

impl Drop for ManagedInteractiveSession {
    fn drop(&mut self) {
        self.output_task.abort();
    }
}

impl InteractiveShellSessionManager {
    pub async fn handle_request(
        &self,
        request: InteractiveShellRequest<'_>,
    ) -> Result<InteractiveShellResult, error::InteractiveShellError> {
        tracing::error!("In the exec");
        // todo update the errors
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        let mut new_session: Option<ManagedInteractiveSession> = None;
        let session_id;
        let writer_tx;
        let output_buffer;
        let output_notify;

        if let Some(existing_id) = request.session_id {
            let sessions = self.sessions.lock().await;
            match sessions.get(&existing_id) {
                Some(session) => {
                    let (buffer, notify) = session.output_handles();
                    session_id = existing_id;
                    writer_tx = session.writer_sender();
                    output_buffer = buffer;
                    output_notify = notify;
                }
                None => {
                    return Err(error::InteractiveShellError::UnknownSessionId {
                        session_id: existing_id,
                    });
                }
            }
        } else {
            let command = command_from_chunks(request.input_chunks)?;
            let new_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);
            let session = create_shell_session(&command).await?;
            let managed_session = ManagedInteractiveSession::new(session);
            let (buffer, notify) = managed_session.output_handles();
            writer_tx = managed_session.writer_sender();
            output_buffer = buffer;
            output_notify = notify;
            session_id = new_id;
            new_session = Some(managed_session);
        };

        if request.session_id.is_some() {
            let joined_input = join_input_chunks(request.input_chunks);
            if !joined_input.is_empty() && writer_tx.send(joined_input.into_bytes()).await.is_err()
            {
                return Err(error::InteractiveShellError::WriteToStdin);
            }
        }

        let mut collected: Vec<u8> = Vec::with_capacity(4096);
        let start = Instant::now();
        let deadline = start + Duration::from_millis(timeout_ms);

        loop {
            let drained_chunks = {
                let mut guard = output_buffer.lock().await;
                let mut drained = Vec::new();
                while let Some(chunk) = guard.pop_front() {
                    drained.push(chunk);
                }
                drained
            };

            if drained_chunks.is_empty() {
                if Instant::now() >= deadline {
                    break;
                }

                let remaining = deadline.saturating_duration_since(Instant::now());
                tokio::select! {
                    _ = output_notify.notified() => {}
                    _ = tokio::time::sleep(remaining) => break,
                }
            } else {
                for chunk in drained_chunks {
                    collected.extend_from_slice(&chunk);
                }

                if Instant::now() >= deadline {
                    break;
                }
            }
        }

        let (output, _maybe_tokens) = truncate_middle(
            &String::from_utf8_lossy(&collected),
            ISHELL_OUTPUT_MAX_BYTES,
        );

        let should_store_session = if let Some(session) = new_session.as_ref() {
            !session.has_exited()
        } else {
            true
        };

        if should_store_session {
            if let Some(session) = new_session {
                self.sessions.lock().await.insert(session_id, session);
            }
            Ok(InteractiveShellResult {
                session_id: Some(session_id),
                output,
            })
        } else {
            Ok(InteractiveShellResult {
                session_id: None,
                output,
            })
        }
    }
}

pub(crate) fn parse_command_line(line: &str) -> Result<Vec<String>, error::InteractiveShellError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(error::InteractiveShellError::MissingCommandLine);
    }

    match shlex::split(trimmed) {
        Some(parts) if !parts.is_empty() => Ok(parts),
        _ => Err(error::InteractiveShellError::InvalidCommandLine {
            command_line: trimmed.to_string(),
        }),
    }
}

async fn create_shell_session(
    command: &[String],
) -> Result<ExecCommandSession, error::InteractiveShellError> {
    if command.is_empty() {
        return Err(error::InteractiveShellError::MissingCommandLine);
    }

    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(error::InteractiveShellError::create_session)?;

    let resolved_command = resolve_command_path(&command[0])?;
    let mut command_builder = CommandBuilder::new(&resolved_command);
    for arg in &command[1..] {
        command_builder.arg(arg);
    }

    let mut child = pair
        .slave
        .spawn_command(command_builder)
        .map_err(error::InteractiveShellError::create_session)?;
    let killer = child.clone_killer();

    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(128);
    let (output_tx, _) = tokio::sync::broadcast::channel::<Vec<u8>>(256);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(error::InteractiveShellError::create_session)?;
    let output_tx_clone = output_tx.clone();
    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = output_tx_clone.send(buf[..n].to_vec());
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => break,
            }
        }
    });

    let writer = pair
        .master
        .take_writer()
        .map_err(error::InteractiveShellError::create_session)?;
    let writer = Arc::new(StdMutex::new(writer));
    let writer_handle = tokio::spawn({
        let writer = writer.clone();
        async move {
            while let Some(bytes) = writer_rx.recv().await {
                let writer = writer.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut guard) = writer.lock() {
                        use std::io::Write;
                        let _ = guard.write_all(&bytes);
                        let _ = guard.flush();
                    }
                })
                .await;
            }
        }
    });

    let exit_status = Arc::new(AtomicBool::new(false));
    let wait_exit_status = Arc::clone(&exit_status);
    let wait_handle = tokio::task::spawn_blocking(move || {
        let _ = child.wait();
        wait_exit_status.store(true, Ordering::SeqCst);
    });

    Ok(ExecCommandSession::new(
        writer_tx,
        output_tx,
        killer,
        reader_handle,
        writer_handle,
        wait_handle,
        exit_status,
    ))
}

fn resolve_command_path(command: &str) -> Result<String, error::InteractiveShellError> {
    if command.is_empty() {
        return Err(error::InteractiveShellError::MissingCommandLine);
    }

    if is_explicit_path(command) {
        return ensure_executable(Path::new(command))
            .then_some(command.to_string())
            .ok_or_else(|| error::InteractiveShellError::CommandNotFound {
                command: command.to_string(),
            });
    }

    if let Some(resolved) = find_in_path(command) {
        return Ok(resolved.to_string_lossy().to_string());
    }

    Err(error::InteractiveShellError::CommandNotFound {
        command: command.to_string(),
    })
}

fn command_from_chunks(chunks: &[String]) -> Result<Vec<String>, error::InteractiveShellError> {
    match chunks {
        [] => Err(error::InteractiveShellError::MissingCommandLine),
        [single] => parse_command_line(single),
        _ => Ok(chunks.to_vec()),
    }
}

fn join_input_chunks(chunks: &[String]) -> String {
    match chunks {
        [] => String::new(),
        [single] => single.clone(),
        _ => chunks.concat(),
    }
}

fn is_explicit_path(command: &str) -> bool {
    let path = Path::new(command);
    path.is_absolute() || path.components().count() > 1
}

fn find_in_path(command: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .flat_map(|dir| candidate_paths(dir, command))
        .find(|candidate| ensure_executable(candidate))
}

fn candidate_paths(dir: PathBuf, command: &str) -> Vec<PathBuf> {
    build_platform_candidates(dir.join(command))
}

#[cfg(unix)]
fn build_platform_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    vec![candidate]
}

#[cfg(windows)]
fn build_platform_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    if candidate.extension().is_some() {
        return vec![candidate];
    }

    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut candidates = Vec::new();
    for ext in pathext.split(';') {
        if ext.is_empty() {
            continue;
        }
        let mut path_with_ext = candidate.clone();
        let new_ext = ext.trim_start_matches('.');
        path_with_ext.set_extension(new_ext);
        candidates.push(path_with_ext);
    }
    if candidates.is_empty() {
        candidates.push(candidate);
    }
    candidates
}

fn ensure_executable(path: &Path) -> bool {
    match path.metadata() {
        Ok(metadata) => metadata.is_file() && is_executable(&metadata),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file()
}

/// Truncate the middle of a UTF-8 string to at most `max_bytes` bytes,
/// preserving the beginning and the end. Returns the possibly truncated
/// string and `Some(original_token_count)` (estimated at 4 bytes/token)
/// if truncation occurred; otherwise returns the original string and `None`.
fn truncate_middle(s: &str, max_bytes: usize) -> (String, Option<u64>) {
    if s.len() <= max_bytes {
        return (s.to_string(), None);
    }
    let est_tokens = (s.len() as u64).div_ceil(4);
    if max_bytes == 0 {
        return (format!("…{est_tokens} tokens truncated…"), Some(est_tokens));
    }

    fn truncate_on_boundary(input: &str, max_len: usize) -> &str {
        if input.len() <= max_len {
            return input;
        }
        let mut end = max_len;
        while end > 0 && !input.is_char_boundary(end) {
            end -= 1;
        }
        &input[..end]
    }

    fn pick_prefix_end(s: &str, left_budget: usize) -> usize {
        if let Some(head) = s.get(..left_budget)
            && let Some(i) = head.rfind('\n')
        {
            return i + 1;
        }
        truncate_on_boundary(s, left_budget).len()
    }

    fn pick_suffix_start(s: &str, right_budget: usize) -> usize {
        let start_tail = s.len().saturating_sub(right_budget);
        if let Some(tail) = s.get(start_tail..)
            && let Some(i) = tail.find('\n')
        {
            return start_tail + i + 1;
        }
        let mut idx = start_tail.min(s.len());
        while idx < s.len() && !s.is_char_boundary(idx) {
            idx += 1;
        }
        idx
    }

    let mut guess_tokens = est_tokens;
    for _ in 0..4 {
        let marker = format!("…{guess_tokens} tokens truncated…");
        let marker_len = marker.len();
        let keep_budget = max_bytes.saturating_sub(marker_len);
        if keep_budget == 0 {
            return (format!("…{est_tokens} tokens truncated…"), Some(est_tokens));
        }

        let left_budget = keep_budget / 2;
        let right_budget = keep_budget - left_budget;
        let prefix_end = pick_prefix_end(s, left_budget);
        let mut suffix_start = pick_suffix_start(s, right_budget);
        if suffix_start < prefix_end {
            suffix_start = prefix_end;
        }
        let kept_content_bytes = prefix_end + (s.len() - suffix_start);
        let truncated_content_bytes = s.len().saturating_sub(kept_content_bytes);
        let new_tokens = (truncated_content_bytes as u64).div_ceil(4);
        if new_tokens == guess_tokens {
            let mut out = String::with_capacity(marker_len + kept_content_bytes + 1);
            out.push_str(&s[..prefix_end]);
            out.push_str(&marker);
            out.push('\n');
            out.push_str(&s[suffix_start..]);
            return (out, Some(est_tokens));
        }
        guess_tokens = new_tokens;
    }

    let marker = format!("…{guess_tokens} tokens truncated…");
    let marker_len = marker.len();
    let keep_budget = max_bytes.saturating_sub(marker_len);
    if keep_budget == 0 {
        return (format!("…{est_tokens} tokens truncated…"), Some(est_tokens));
    }
    let left_budget = keep_budget / 2;
    let right_budget = keep_budget - left_budget;
    let prefix_end = pick_prefix_end(s, left_budget);
    let suffix_start = pick_suffix_start(s, right_budget);
    let mut out = String::with_capacity(marker_len + prefix_end + (s.len() - suffix_start) + 1);
    out.push_str(&s[..prefix_end]);
    out.push_str(&marker);
    out.push('\n');
    out.push_str(&s[suffix_start..]);
    (out, Some(est_tokens))
}

mod error {
    #[derive(Debug, thiserror::Error)]
    pub(crate) enum InteractiveShellError {
        #[error("Failed to create interactive shell: {pty_error}")]
        CreateSession {
            #[source]
            pty_error: anyhow::Error,
        },
        #[error("Unknown session id {session_id}")]
        UnknownSessionId { session_id: i32 },
        #[error("failed to write to stdin")]
        WriteToStdin,
        #[error("missing command line for interactive shell request")]
        MissingCommandLine,
        #[error("invalid command line: {command_line}")]
        InvalidCommandLine { command_line: String },
        #[error("command not found: {command}")]
        CommandNotFound { command: String },
    }

    impl InteractiveShellError {
        pub(crate) fn create_session(error: anyhow::Error) -> Self {
            Self::CreateSession { pty_error: error }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_line_splits_words() {
        assert_eq!(
            parse_command_line("echo codex").unwrap(),
            vec!["echo".to_string(), "codex".to_string()]
        );
    }

    #[test]
    fn parse_command_line_trims_whitespace() {
        assert_eq!(
            parse_command_line("  ls  -la  \n").unwrap(),
            vec!["ls".to_string(), "-la".to_string()]
        );
    }

    #[test]
    fn parse_command_line_rejects_empty() {
        let err = parse_command_line("   ").expect_err("expected error");
        assert!(matches!(
            err,
            error::InteractiveShellError::MissingCommandLine
        ));
    }

    #[cfg(unix)]
    /// Ensures that environment state persists when reusing the same
    /// interactive shell session across multiple requests.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn interactive_shell_persists_across_requests() -> Result<(), error::InteractiveShellError>
    {
        let manager = InteractiveShellSessionManager::default();

        let open_shell = manager
            .handle_request(InteractiveShellRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_id = open_shell.session_id.expect("expected session_id");

        manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_id),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_id),
                input_chunks: &["echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        assert!(out_2.output.contains("codex"));

        Ok(())
    }

    #[cfg(unix)]
    /// Verifies independent shell sessions maintain separate state while
    /// previously created sessions continue to function.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn multi_interactive_shells() -> Result<(), error::InteractiveShellError> {
        let manager = InteractiveShellSessionManager::default();

        let shell_a = manager
            .handle_request(InteractiveShellRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_a = shell_a.session_id.expect("expected session id");

        manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_a),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(InteractiveShellRequest {
                session_id: None,
                input_chunks: &["/bin/echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        assert!(!out_2.output.contains("codex"));

        let out_3 = manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_a),
                input_chunks: &["echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        assert!(out_3.output.contains("codex"));

        Ok(())
    }

    // todo add a test for the path

    #[cfg(unix)]
    /// Confirms that output emitted after an initial request times out can be
    /// collected by a follow-up request against the same session.
    #[tokio::test]
    async fn interactive_shell_timeouts() -> Result<(), error::InteractiveShellError> {
        let manager = InteractiveShellSessionManager::default();

        let open_shell = manager
            .handle_request(InteractiveShellRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_id = open_shell.session_id.expect("expected session id");

        manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_id),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_id),
                input_chunks: &["sleep 5 && echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(10),
            })
            .await?;
        assert!(!out_2.output.contains("codex"));

        // Wait for the end of the bash sleep (preventing the usage of tokio controlled clock).
        tokio::time::sleep(Duration::from_secs(7)).await;

        let empty = Vec::new();
        let out_3 = manager
            .handle_request(InteractiveShellRequest {
                session_id: Some(session_id),
                input_chunks: &empty,
                timeout_ms: Some(100),
            })
            .await?;

        assert!(out_3.output.contains("codex"));

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    /// Ensure that commands which immediately complete do not create persistent
    /// interactive shell sessions, preventing leftover empty sessions from
    /// accumulating.
    async fn completed_commands_do_not_persist_sessions() -> Result<(), error::InteractiveShellError>
    {
        let manager = InteractiveShellSessionManager::default();
        let result = manager
            .handle_request(InteractiveShellRequest {
                session_id: None,
                input_chunks: &["/bin/echo".to_string(), "codex".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        assert!(result.session_id.is_none());
        assert!(result.output.contains("codex"));

        assert!(manager.sessions.lock().await.is_empty());

        Ok(())
    }

    #[test]
    fn truncate_middle_no_newlines_fallback() {
        // todo double check the truncation logic
        // Long string without newlines forces a pure byte/char-boundary truncation.
        let s = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let max_bytes = 16; // force truncation
        let (out, original) = truncate_middle(s, max_bytes);
        // For very small caps, we return a full, untruncated marker that may exceed the cap.
        assert_eq!(out, "…16 tokens truncated…"); // todo rename the token word
        // Original is ceil(62/4) = 16 tokens.
        assert_eq!(original, Some(16));
    }

    #[test]
    fn truncate_middle_prefers_newline_boundaries() {
        // Build a multi-line string of 20 numbered lines (each "NNN\n").
        let mut s = String::new();
        for i in 1..=20 {
            s.push_str(&format!("{i:03}\n"));
        }
        assert_eq!(s.len(), 80);

        let max_bytes = 64; // force truncation while leaving room for head/tail
        assert_eq!(
            truncate_middle(&s, max_bytes),
            (
                "001\n002\n003\n004\n…12 tokens truncated…\n017\n018\n019\n020\n".to_string(),
                Some(20)
            )
        );
    }
}
