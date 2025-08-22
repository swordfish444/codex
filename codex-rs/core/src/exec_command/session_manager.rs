use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicU32;

use portable_pty::CommandBuilder;
use portable_pty::PtySize;
use portable_pty::native_pty_system;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::timeout;

use crate::exec_command::exec_command_params::ExecCommandParams;
use crate::exec_command::exec_command_params::WriteStdinParams;
use crate::exec_command::exec_command_session::ExecCommandSession;
use crate::exec_command::session_id::SessionId;
use crate::models::FunctionCallOutputPayload;
use crate::models::ResponseInputItem;

pub static SESSION_MANAGER: LazyLock<SessionManager> = LazyLock::new(SessionManager::default);

#[derive(Debug, Default)]
pub struct SessionManager {
    next_session_id: AtomicU32,
    sessions: Mutex<HashMap<SessionId, ExecCommandSession>>,
}

impl SessionManager {
    /// Processes the request and is required to send a response via `outgoing`.
    pub async fn handle_exec_command_request(
        &self,
        call_id: String,
        params: ExecCommandParams,
    ) -> ResponseInputItem {
        // Allocate a session id.
        let session_id = SessionId(
            self.next_session_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );

        let result = create_exec_command_session(session_id, params.clone()).await;

        match result {
            Ok((session, mut exit_rx)) => {
                // Insert into session map.
                let output_receiver = session.output_receiver();
                self.sessions.lock().await.insert(session_id, session);

                // Collect output until either timeout expires or process exits.
                // Cap by assuming 4 bytes per token (TODO: use a real tokenizer).
                let cap_bytes_u64 = params.max_output_tokens.saturating_mul(4);
                let cap_bytes: usize = cap_bytes_u64.min(usize::MAX as u64) as usize;
                let cap_hint = cap_bytes.clamp(1024, 8192);
                let mut collected: Vec<u8> = Vec::with_capacity(cap_hint);

                let deadline = Instant::now() + Duration::from_millis(params.yield_time_ms);
                let mut exit_code: Option<i32> = None;

                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    tokio::select! {
                        biased;
                        exit = &mut exit_rx => {
                            exit_code = exit.ok();
                            // Small grace period to pull remaining buffered output
                            let grace_deadline = Instant::now() + Duration::from_millis(25);
                            while Instant::now() < grace_deadline {
                                let recv_next = async {
                                    let mut rx = output_receiver.lock().await;
                                    rx.recv().await
                                };
                                if let Ok(Some(chunk)) = timeout(Duration::from_millis(1), recv_next).await {
                                    let available = cap_bytes.saturating_sub(collected.len());
                                    if available == 0 { break; }
                                    let take = available.min(chunk.len());
                                    collected.extend_from_slice(&chunk[..take]);
                                } else {
                                    break;
                                }
                            }
                            break;
                        }
                        chunk = timeout(remaining, async {
                            let mut rx = output_receiver.lock().await;
                            rx.recv().await
                        }) => {
                            match chunk {
                                Ok(Some(chunk)) => {
                                    let available = cap_bytes.saturating_sub(collected.len());
                                    if available == 0 { /* keep draining, but don't store */ }
                                    else {
                                        let take = available.min(chunk.len());
                                        collected.extend_from_slice(&chunk[..take]);
                                    }
                                }
                                Ok(None) => { break; }
                                Err(_) => { break; }
                            }
                        }
                    }
                }

                let text = String::from_utf8_lossy(&collected).to_string();
                let mut structured = json!({ "sessionId": session_id });
                if let Some(code) = exit_code {
                    structured["exitCode"] = json!(code);
                }

                ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload {
                        content: text,
                        success: Some(true),
                    },
                }
            }
            Err(err) => ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to start exec session: {err}"),
                    success: Some(false),
                },
            },
        }
    }

    /// Write characters to a session's stdin and collect combined output for up to `yield_time_ms`.
    pub async fn handle_write_stdin_request(
        &self,
        call_id: String,
        params: WriteStdinParams,
    ) -> ResponseInputItem {
        let WriteStdinParams {
            session_id,
            chars,
            yield_time_ms,
            max_output_tokens,
        } = params;

        // Grab handles without holding the sessions lock across await points.
        let (writer_tx, output_rx) = {
            let sessions = self.sessions.lock().await;
            match sessions.get(&session_id) {
                Some(session) => (session.writer_sender(), session.output_receiver()),
                None => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            content: format!("unknown session id {}", session_id.0),
                            success: Some(false),
                        },
                    };
                }
            }
        };

        // Write stdin if provided.
        if !chars.is_empty() && writer_tx.send(chars.into_bytes()).await.is_err() {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "failed to write to stdin".to_string(),
                    success: Some(false),
                },
            };
        }

        // Collect output up to yield_time_ms, truncating to max_output_tokens bytes.
        let mut collected: Vec<u8> = Vec::with_capacity(4096);
        let deadline = Instant::now() + Duration::from_millis(yield_time_ms);
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            match timeout(remaining, output_rx.lock().await.recv()).await {
                Ok(Some(chunk)) => {
                    // Respect token/byte limit; keep draining but drop once full.
                    let available =
                        max_output_tokens.saturating_sub(collected.len() as u64) as usize;
                    if available > 0 {
                        let take = available.min(chunk.len());
                        collected.extend_from_slice(&chunk[..take]);
                    }
                    // Continue loop to drain further within time.
                }
                Ok(None) => break, // channel closed
                Err(_) => break,   // timeout
            }
        }

        // Return text output as a CallToolResult
        let text = String::from_utf8_lossy(&collected).to_string();
        ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: text,
                success: Some(true),
            },
        }
    }
}

/// Spawn PTY and child process per spawn_exec_command_session logic.
async fn create_exec_command_session(
    session_id: SessionId,
    params: ExecCommandParams,
) -> anyhow::Result<(ExecCommandSession, oneshot::Receiver<i32>)> {
    let ExecCommandParams {
        cmd,
        yield_time_ms: _,
        max_output_tokens: _,
        shell,
        login,
    } = params;

    // Use the native pty implementation for the system
    let pty_system = native_pty_system();

    // Create a new pty
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Spawn a shell into the pty
    let mut command_builder = CommandBuilder::new(shell);
    let shell_mode_opt = if login { "-lc" } else { "-c" };
    command_builder.arg(shell_mode_opt);
    command_builder.arg(cmd);

    let mut child = pair.slave.spawn_command(command_builder)?;

    // Channel to forward write requests to the PTY writer.
    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(128);
    // Channel for streaming PTY output to readers.
    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);

    // Reader task: drain PTY and forward chunks to output channel.
    let mut reader = pair.master.try_clone_reader()?;
    let output_tx_clone = output_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    // Forward; block if receiver is slow to avoid dropping output.
                    let _ = output_tx_clone.blocking_send(buf[..n].to_vec());
                }
                Err(_) => break,
            }
        }
    });

    // Writer task: apply stdin writes to the PTY writer.
    let writer = pair.master.take_writer()?;
    let writer = Arc::new(StdMutex::new(writer));
    tokio::spawn({
        let writer = writer.clone();
        async move {
            while let Some(bytes) = writer_rx.recv().await {
                let writer = writer.clone();
                // Perform blocking write on a blocking thread.
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

    // Keep the child alive until it exits, then signal exit code.
    let (exit_tx, exit_rx) = oneshot::channel::<i32>();
    tokio::task::spawn_blocking(move || {
        let code = match child.wait() {
            Ok(status) => status.exit_code() as i32,
            Err(_) => -1,
        };
        let _ = exit_tx.send(code);
    });

    // Create and store the session with channels.
    let session = ExecCommandSession::new(session_id, writer_tx, output_rx);
    Ok((session, exit_rx))
}
