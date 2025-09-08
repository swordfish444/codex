use portable_pty::CommandBuilder;
use portable_pty::PtySize;
use portable_pty::native_pty_system;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::io::Read;
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

mod errors;
mod path;
mod truncation;

pub(crate) use errors::UnifiedExecError;

use path::command_from_chunks;
use path::join_input_chunks;
use path::resolve_command_path;
use truncation::truncate_middle;

const DEFAULT_TIMEOUT_MS: u64 = 250;
const UNIFIED_EXEC_OUTPUT_MAX_BYTES: usize = 16 * 1024; // 16 KiB

#[derive(Debug)]
pub(crate) struct UnifiedExecRequest<'a> {
    pub session_id: Option<i32>,
    pub input_chunks: &'a [String],
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnifiedExecResult {
    pub session_id: Option<i32>,
    pub output: String,
}

#[derive(Debug, Default)]
pub(crate) struct UnifiedExecSessionManager {
    next_session_id: AtomicI32,
    sessions: Mutex<HashMap<i32, ManagedUnifiedExecSession>>,
}

#[derive(Debug)]
struct ManagedUnifiedExecSession {
    session: ExecCommandSession,
    output_buffer: OutputBuffer,
    output_notify: Arc<Notify>,
    output_task: JoinHandle<()>,
}

type OutputBuffer = Arc<Mutex<VecDeque<Vec<u8>>>>;
type OutputHandles = (OutputBuffer, Arc<Notify>);

impl ManagedUnifiedExecSession {
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

impl Drop for ManagedUnifiedExecSession {
    fn drop(&mut self) {
        self.output_task.abort();
    }
}

impl UnifiedExecSessionManager {
    pub async fn handle_request(
        &self,
        request: UnifiedExecRequest<'_>,
    ) -> Result<UnifiedExecResult, UnifiedExecError> {
        tracing::error!("In the exec");
        // todo update the errors
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        let mut new_session: Option<ManagedUnifiedExecSession> = None;
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
                    return Err(UnifiedExecError::UnknownSessionId {
                        session_id: existing_id,
                    });
                }
            }
        } else {
            let command = command_from_chunks(request.input_chunks)?;
            let new_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);
            let session = create_unified_exec_session(&command).await?;
            let managed_session = ManagedUnifiedExecSession::new(session);
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
                return Err(UnifiedExecError::WriteToStdin);
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
            UNIFIED_EXEC_OUTPUT_MAX_BYTES,
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
            Ok(UnifiedExecResult {
                session_id: Some(session_id),
                output,
            })
        } else {
            Ok(UnifiedExecResult {
                session_id: None,
                output,
            })
        }
    }
}

async fn create_unified_exec_session(
    command: &[String],
) -> Result<ExecCommandSession, UnifiedExecError> {
    if command.is_empty() {
        return Err(UnifiedExecError::MissingCommandLine);
    }

    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(UnifiedExecError::create_session)?;

    let resolved_command = resolve_command_path(&command[0])?;
    let mut command_builder = CommandBuilder::new(&resolved_command);
    for arg in &command[1..] {
        command_builder.arg(arg);
    }

    let mut child = pair
        .slave
        .spawn_command(command_builder)
        .map_err(UnifiedExecError::create_session)?;
    let killer = child.clone_killer();

    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(128);
    let (output_tx, _) = tokio::sync::broadcast::channel::<Vec<u8>>(256);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(UnifiedExecError::create_session)?;
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
        .map_err(UnifiedExecError::create_session)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::path::parse_command_line;

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
        assert!(matches!(err, UnifiedExecError::MissingCommandLine));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unified_exec_persists_across_requests() -> Result<(), UnifiedExecError> {
        let manager = UnifiedExecSessionManager::default();

        let open_shell = manager
            .handle_request(UnifiedExecRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_id = open_shell.session_id.expect("expected session_id");

        manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_id),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_id),
                input_chunks: &["echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        assert!(out_2.output.contains("codex"));

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn multi_unified_exec_sessions() -> Result<(), UnifiedExecError> {
        let manager = UnifiedExecSessionManager::default();

        let shell_a = manager
            .handle_request(UnifiedExecRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_a = shell_a.session_id.expect("expected session id");

        manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_a),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(UnifiedExecRequest {
                session_id: None,
                input_chunks: &["/bin/echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        assert!(!out_2.output.contains("codex"));

        let out_3 = manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_a),
                input_chunks: &["echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        assert!(out_3.output.contains("codex"));

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unified_exec_timeouts() -> Result<(), UnifiedExecError> {
        let manager = UnifiedExecSessionManager::default();

        let open_shell = manager
            .handle_request(UnifiedExecRequest {
                session_id: None,
                input_chunks: &["/bin/bash".to_string(), "-i".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;
        let session_id = open_shell.session_id.expect("expected session id");

        manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_id),
                input_chunks: &["export CODEX_INTERACTIVE_SHELL_VAR=codex\n".to_string()],
                timeout_ms: Some(1_500),
            })
            .await?;

        let out_2 = manager
            .handle_request(UnifiedExecRequest {
                session_id: Some(session_id),
                input_chunks: &["sleep 5 && echo $CODEX_INTERACTIVE_SHELL_VAR\n".to_string()],
                timeout_ms: Some(10),
            })
            .await?;
        assert!(!out_2.output.contains("codex"));

        tokio::time::sleep(Duration::from_secs(7)).await;

        let empty = Vec::new();
        let out_3 = manager
            .handle_request(UnifiedExecRequest {
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
    async fn completed_commands_do_not_persist_sessions() -> Result<(), UnifiedExecError> {
        let manager = UnifiedExecSessionManager::default();
        let result = manager
            .handle_request(UnifiedExecRequest {
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
        let s = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let max_bytes = 16;
        let (out, original) = truncate_middle(s, max_bytes);
        assert_eq!(out, "…16 tokens truncated…");
        assert_eq!(original, Some(16));
    }

    #[test]
    fn truncate_middle_prefers_newline_boundaries() {
        let mut s = String::new();
        for i in 1..=20 {
            s.push_str(&format!("{i:03}\n"));
        }
        assert_eq!(s.len(), 80);

        let max_bytes = 64;
        assert_eq!(
            truncate_middle(&s, max_bytes),
            (
                "001\n002\n003\n004\n…12 tokens truncated…\n017\n018\n019\n020\n".to_string(),
                Some(20)
            )
        );
    }
}
