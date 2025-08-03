use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use codex_core::codex_wrapper::CodexConversation;
use codex_core::codex_wrapper::init_codex;
use codex_core::config::Config;
use codex_core::protocol::AgentMessageDeltaEvent;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::AgentReasoningDeltaEvent;
use codex_core::protocol::AgentReasoningEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::ErrorEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::InputItem;
use codex_core::protocol::McpToolCallBeginEvent;
use codex_core::protocol::McpToolCallEndEvent;
use codex_core::protocol::Op;
use codex_core::protocol::PatchApplyBeginEvent;
use codex_core::protocol::TaskCompleteEvent;
use codex_core::protocol::TokenUsage;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::InputResult;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::history_cell::CommandOutput;
use crate::history_cell::HistoryCell;
use crate::history_cell::PatchEventType;
use crate::user_approval_widget::ApprovalRequest;
use codex_file_search::FileMatch;

struct RunningCommand {
    command: Vec<String>,
    #[allow(dead_code)]
    cwd: PathBuf,
}

pub(crate) struct ChatWidget<'a> {
    app_event_tx: AppEventSender,
    codex_op_tx: UnboundedSender<Op>,
    bottom_pane: BottomPane<'a>,
    config: Config,
    initial_user_message: Option<UserMessage>,
    token_usage: TokenUsage,
    reasoning_buffer: String,
    // Buffer for streaming assistant answer text; we do not surface partial
    // We wait for the final AgentMessage event and then emit the full text
    // at once into scrollback so the history contains a single message.
    answer_buffer: String,
    running_commands: HashMap<String, RunningCommand>,
    pending_commits: VecDeque<PendingHistoryCommit>,
    queued_status_text: Option<String>,
    defer_task_stop: bool,
}

struct UserMessage {
    text: String,
    image_paths: Vec<PathBuf>,
}

enum PendingHistoryCommit {
    AgentMessage(String),
    AgentReasoning(String),
    /// Generic deferred history commit with a preview string to animate
    /// in the live cell before committing the full `HistoryCell` to
    /// scrollback.
    HistoryCellWithPreview {
        cell: HistoryCell,
        preview: String,
    },
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            image_paths: Vec::new(),
        }
    }
}

fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        Some(UserMessage { text, image_paths })
    }
}

impl ChatWidget<'_> {
    pub(crate) fn new(
        config: Config,
        app_event_tx: AppEventSender,
        initial_prompt: Option<String>,
        initial_images: Vec<PathBuf>,
        enhanced_keys_supported: bool,
    ) -> Self {
        let (codex_op_tx, mut codex_op_rx) = unbounded_channel::<Op>();

        let app_event_tx_clone = app_event_tx.clone();
        // Create the Codex asynchronously so the UI loads as quickly as possible.
        let config_for_agent_loop = config.clone();
        tokio::spawn(async move {
            let CodexConversation {
                codex,
                session_configured,
                ..
            } = match init_codex(config_for_agent_loop).await {
                Ok(vals) => vals,
                Err(e) => {
                    // TODO: surface this error to the user.
                    tracing::error!("failed to initialize codex: {e}");
                    return;
                }
            };

            // Forward the captured `SessionInitialized` event that was consumed
            // inside `init_codex()` so it can be rendered in the UI.
            app_event_tx_clone.send(AppEvent::CodexEvent(session_configured.clone()));
            let codex = Arc::new(codex);
            let codex_clone = codex.clone();
            tokio::spawn(async move {
                while let Some(op) = codex_op_rx.recv().await {
                    let id = codex_clone.submit(op).await;
                    if let Err(e) = id {
                        tracing::error!("failed to submit op: {e}");
                    }
                }
            });

            while let Ok(event) = codex.next_event().await {
                app_event_tx_clone.send(AppEvent::CodexEvent(event));
            }
        });

        Self {
            app_event_tx: app_event_tx.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
            }),
            config,
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            token_usage: TokenUsage::default(),
            reasoning_buffer: String::new(),
            answer_buffer: String::new(),
            running_commands: HashMap::new(),
            pending_commits: VecDeque::new(),
            queued_status_text: None,
            defer_task_stop: false,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane.desired_height(width)
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Press {
            self.bottom_pane.clear_ctrl_c_quit_hint();
        }

        match self.bottom_pane.handle_key_event(key_event) {
            InputResult::Submitted(text) => {
                self.submit_user_message(text.into());
            }
            InputResult::None => {}
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        self.bottom_pane.handle_paste(text);
    }

    fn add_to_history(&mut self, cell: HistoryCell) {
        self.app_event_tx
            .send(AppEvent::InsertHistory(cell.plain_lines()));
    }

    /// Queue a history cell to be inserted after the current typewriter
    /// animation completes. If no animation is active, start one now.
    fn queue_commit_with_preview(&mut self, cell: HistoryCell, preview: String) {
        self.pending_commits
            .push_back(PendingHistoryCommit::HistoryCellWithPreview {
                cell,
                preview: preview.clone(),
            });
        if self.pending_commits.len() == 1 {
            self.bottom_pane.restart_live_status_with_text(preview);
        }
        self.request_redraw();
    }

    fn submit_user_message(&mut self, user_message: UserMessage) {
        let UserMessage { text, image_paths } = user_message;
        let mut items: Vec<InputItem> = Vec::new();

        if !text.is_empty() {
            items.push(InputItem::Text { text: text.clone() });
        }

        for path in image_paths {
            items.push(InputItem::LocalImage { path });
        }

        if items.is_empty() {
            return;
        }

        self.codex_op_tx
            .send(Op::UserInput { items })
            .unwrap_or_else(|e| {
                tracing::error!("failed to send message: {e}");
            });

        // Persist the text to cross-session message history.
        if !text.is_empty() {
            self.codex_op_tx
                .send(Op::AddToHistory { text: text.clone() })
                .unwrap_or_else(|e| {
                    tracing::error!("failed to send AddHistory op: {e}");
                });
        }

        // Only show text portion in conversation history for now.
        if !text.is_empty() {
            self.add_to_history(HistoryCell::new_user_prompt(text.clone()));
        }
    }

    pub(crate) fn handle_codex_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        match msg {
            EventMsg::SessionConfigured(event) => {
                self.bottom_pane
                    .set_history_metadata(event.history_log_id, event.history_entry_count);
                // Record session information at the top of the conversation.
                self.add_to_history(HistoryCell::new_session_info(&self.config, event, true));

                if let Some(user_message) = self.initial_user_message.take() {
                    // If the user provided an initial message, add it to the
                    // conversation history.
                    self.submit_user_message(user_message);
                }

                self.request_redraw();
            }
            EventMsg::AgentMessage(AgentMessageEvent { message }) => {
                // Final assistant answer. Prefer the fully provided message
                // from the event; if it is empty fall back to any accumulated
                // delta buffer (some providers may only stream deltas and send
                // an empty final message).
                let full = if message.is_empty() {
                    std::mem::take(&mut self.answer_buffer)
                } else {
                    self.answer_buffer.clear();
                    message
                };
                if !full.is_empty() {
                    // Queue for commit; if this is the only pending item start
                    // the typewriter animation now so we wait before committing.
                    self.pending_commits
                        .push_back(PendingHistoryCommit::AgentMessage(full.clone()));
                    if self.pending_commits.len() == 1 {
                        self.bottom_pane.restart_live_status_with_text(full);
                    }
                }
                self.request_redraw();
            }
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                // Stream partial assistant output live: update the in‑pane
                // status view with the growing answer so the user sees typing
                // feedback immediately. We still insert the final message as
                // a single history entry on AgentMessage.
                self.answer_buffer.push_str(&delta);
                self.update_latest_log(self.answer_buffer.clone());
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
                // Buffer only – disable incremental reasoning streaming so we
                // avoid truncated intermediate lines. Full text emitted on
                // AgentReasoning.
                self.reasoning_buffer.push_str(&delta);
                // Animate chain-of-thought live in the status indicator.
                self.update_latest_log(self.reasoning_buffer.clone());
            }
            EventMsg::AgentReasoning(AgentReasoningEvent { text }) => {
                // Emit full reasoning text once. Some providers might send
                // final event with empty text if only deltas were used.
                let full = if text.is_empty() {
                    std::mem::take(&mut self.reasoning_buffer)
                } else {
                    self.reasoning_buffer.clear();
                    text
                };
                if !full.is_empty() {
                    self.pending_commits
                        .push_back(PendingHistoryCommit::AgentReasoning(full.clone()));
                    if self.pending_commits.len() == 1 {
                        self.bottom_pane.restart_live_status_with_text(full);
                    }
                }
                self.request_redraw();
            }
            EventMsg::TaskStarted => {
                self.bottom_pane.clear_ctrl_c_quit_hint();
                self.bottom_pane.set_task_running(true);
                self.request_redraw();
            }
            EventMsg::TaskComplete(TaskCompleteEvent {
                last_agent_message: _,
            }) => {
                if self.pending_commits.is_empty() {
                    self.bottom_pane.set_task_running(false);
                } else {
                    // Defer stopping the task UI until after the final
                    // animated commit has been written to history.
                    self.defer_task_stop = true;
                }
                self.request_redraw();
            }
            EventMsg::TokenCount(token_usage) => {
                self.token_usage = add_token_usage(&self.token_usage, &token_usage);
                self.bottom_pane
                    .set_token_usage(self.token_usage.clone(), self.config.model_context_window);
            }
            EventMsg::Error(ErrorEvent { message }) => {
                self.add_to_history(HistoryCell::new_error_event(message.clone()));
                self.bottom_pane.set_task_running(false);
            }
            EventMsg::PlanUpdate(update) => {
                let preview = "plan updated".to_string();
                let cell = HistoryCell::new_plan_update(update);
                self.queue_commit_with_preview(cell, preview);
            }
            EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                call_id: _,
                command,
                cwd,
                reason,
            }) => {
                // Queue the command summary; animate it first, then commit it
                // to history so the narrative matches the live cell.
                let cmdline = strip_bash_lc_and_escape(&command);
                let text = format!(
                    "command requires approval:\n$ {cmdline}{reason}",
                    reason = reason
                        .as_ref()
                        .map(|r| format!("\n{r}"))
                        .unwrap_or_default()
                );
                self.queue_commit_with_preview(
                    HistoryCell::new_background_event(text.clone()),
                    text,
                );

                let request = ApprovalRequest::Exec {
                    id,
                    command,
                    cwd,
                    reason,
                };
                self.bottom_pane.push_approval_request(request);
                self.request_redraw();
            }
            EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                call_id: _,
                changes,
                reason,
                grant_root,
            }) => {
                // ------------------------------------------------------------------
                // Before we even prompt the user for approval we surface the patch
                // summary in the main conversation so that the dialog appears in a
                // sensible chronological order:
                //   (1) codex → proposes patch (HistoryCell::PendingPatch)
                //   (2) UI → asks for approval (BottomPane)
                // This mirrors how command execution is shown (command begins →
                // approval dialog) and avoids surprising the user with a modal
                // prompt before they have seen *what* is being requested.
                // ------------------------------------------------------------------
                let file_count = changes.len();
                let reason_suffix = reason
                    .as_ref()
                    .map(|r| format!(" – {r}"))
                    .unwrap_or_default();
                let preview =
                    format!("patch approval requested for {file_count} file(s){reason_suffix}");
                self.queue_commit_with_preview(
                    HistoryCell::new_patch_event(PatchEventType::ApprovalRequest, changes),
                    preview,
                );

                // Now surface the approval request in the BottomPane as before.
                let request = ApprovalRequest::ApplyPatch {
                    id,
                    reason,
                    grant_root,
                };
                self.bottom_pane.push_approval_request(request);
                self.request_redraw();
            }
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id,
                command,
                cwd,
            }) => {
                let cmdline = strip_bash_lc_and_escape(&command);
                self.running_commands.insert(
                    call_id,
                    RunningCommand {
                        command: command.clone(),
                        cwd: cwd.clone(),
                    },
                );
                self.queue_commit_with_preview(
                    HistoryCell::new_active_exec_command(command),
                    format!("$ {cmdline}"),
                );
            }
            EventMsg::ExecCommandOutputDelta(_) => {}
            EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                call_id: _,
                auto_approved,
                changes,
            }) => {
                let prefix = if auto_approved {
                    "applying patch (auto-approved)"
                } else {
                    "applying patch"
                };
                self.queue_commit_with_preview(
                    HistoryCell::new_patch_event(
                        PatchEventType::ApplyBegin { auto_approved },
                        changes,
                    ),
                    prefix.to_string(),
                );
            }
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id,
                exit_code,
                stdout,
                stderr,
            }) => {
                // Compute summary before moving stdout into the history cell.
                let summary = if !stdout.trim().is_empty() {
                    stdout.lines().next().unwrap_or("").to_string()
                } else {
                    format!("command exited with code {exit_code}")
                };
                let cmd = self.running_commands.remove(&call_id);
                self.queue_commit_with_preview(
                    HistoryCell::new_completed_exec_command(
                        cmd.map(|cmd| cmd.command).unwrap_or_else(|| vec![call_id]),
                        CommandOutput {
                            exit_code,
                            stdout,
                            stderr,
                            duration: Duration::from_secs(0),
                        },
                    ),
                    summary,
                );
            }
            EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
                call_id: _,
                invocation,
            }) => {
                // Build brief one-line invocation summary before moving `invocation`.
                let args_str = invocation
                    .arguments
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_else(|_| v.to_string()))
                    .unwrap_or_default();
                let server = invocation.server.clone();
                let tool = invocation.tool.clone();
                let preview = format!("MCP {server}.{tool}({args_str})");
                self.queue_commit_with_preview(
                    HistoryCell::new_active_mcp_tool_call(invocation),
                    preview,
                );
            }
            EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                call_id: _,
                duration,
                invocation,
                result,
            }) => {
                self.queue_commit_with_preview(
                    HistoryCell::new_completed_mcp_tool_call(
                        80,
                        invocation,
                        duration,
                        result
                            .as_ref()
                            .map(|r| r.is_error.unwrap_or(false))
                            .unwrap_or(false),
                        result,
                    ),
                    "MCP call finished".to_string(),
                );
            }
            EventMsg::GetHistoryEntryResponse(event) => {
                let codex_core::protocol::GetHistoryEntryResponseEvent {
                    offset,
                    log_id,
                    entry,
                } = event;

                // Inform bottom pane / composer.
                self.bottom_pane
                    .on_history_entry_response(log_id, offset, entry.map(|e| e.text));
            }
            EventMsg::ShutdownComplete => {
                self.app_event_tx.send(AppEvent::ExitRequest);
            }
            event => {
                let text = format!("{event:?}");
                self.add_to_history(HistoryCell::new_background_event(text.clone()));
                self.update_latest_log(text);
            }
        }
    }

    /// Update the live log preview while a task is running.
    pub(crate) fn update_latest_log(&mut self, line: String) {
        // If we have pending commits waiting to be flushed, hold off on
        // updating the live cell so the current entry can finish its animation.
        if !self.pending_commits.is_empty() {
            self.queued_status_text = Some(line);
        } else {
            self.bottom_pane.update_status_text(line);
        }
    }

    fn request_redraw(&mut self) {
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    pub(crate) fn add_diff_output(&mut self, diff_output: String) {
        self.add_to_history(HistoryCell::new_diff_output(diff_output.clone()));
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }

    /// Called by the app when the live status widget has fully revealed its
    /// current text. We then commit the corresponding pending entry to
    /// history and, if another entry is waiting, start animating it next.
    pub(crate) fn on_live_status_reveal_complete(&mut self) {
        if let Some(pending) = self.pending_commits.pop_front() {
            match pending {
                PendingHistoryCommit::AgentMessage(text) => {
                    self.add_to_history(HistoryCell::new_agent_message(&self.config, text));
                }
                PendingHistoryCommit::AgentReasoning(text) => {
                    self.add_to_history(HistoryCell::new_agent_reasoning(&self.config, text));
                }
                PendingHistoryCommit::HistoryCellWithPreview { cell, .. } => {
                    self.add_to_history(cell);
                }
            }
        }

        // If there is another pending entry, start animating it fresh. We do
        // not remove it from the queue yet; we will commit it on the next
        // completion callback.
        if let Some(next) = self.pending_commits.front() {
            let text = match next {
                PendingHistoryCommit::AgentMessage(t) | PendingHistoryCommit::AgentReasoning(t) => {
                    t.clone()
                }
                PendingHistoryCommit::HistoryCellWithPreview { preview, .. } => preview.clone(),
            };
            // Restart the live status so the typewriter begins from the start
            // for the new entry.
            self.bottom_pane.restart_live_status_with_text(text);
        } else if let Some(queued) = self.queued_status_text.take() {
            // No more pending entries – show any queued status update now.
            self.bottom_pane.update_status_text(queued);
        } else {
            // Nothing more to show; clear the live status and, if we deferred
            // the TaskComplete UI update earlier, apply it now.
            self.bottom_pane.clear_live_status();
            // If the live status replaced the composer (input takeover),
            // restore the composer now that there is nothing left to animate.
            self.bottom_pane.clear_status_view();
            if self.defer_task_stop {
                self.bottom_pane.set_task_running(false);
                self.defer_task_stop = false;
            }
        }

        self.request_redraw();
    }

    /// Handle Ctrl-C key press.
    /// Returns CancellationEvent::Handled if the event was consumed by the UI, or
    /// CancellationEvent::Ignored if the caller should handle it (e.g. exit).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        match self.bottom_pane.on_ctrl_c() {
            CancellationEvent::Handled => return CancellationEvent::Handled,
            CancellationEvent::Ignored => {}
        }
        if self.bottom_pane.is_task_running() {
            self.bottom_pane.clear_ctrl_c_quit_hint();
            self.submit_op(Op::Interrupt);
            self.answer_buffer.clear();
            self.reasoning_buffer.clear();
            CancellationEvent::Ignored
        } else if self.bottom_pane.ctrl_c_quit_hint_visible() {
            self.submit_op(Op::Shutdown);
            CancellationEvent::Handled
        } else {
            self.bottom_pane.show_ctrl_c_quit_hint();
            CancellationEvent::Ignored
        }
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.bottom_pane.composer_is_empty()
    }

    /// Forward an `Op` directly to codex.
    pub(crate) fn submit_op(&self, op: Op) {
        if let Err(e) = self.codex_op_tx.send(op) {
            tracing::error!("failed to submit op: {e}");
        }
    }

    pub(crate) fn token_usage(&self) -> &TokenUsage {
        &self.token_usage
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.token_usage = TokenUsage::default();
        self.bottom_pane
            .set_token_usage(self.token_usage.clone(), self.config.model_context_window);
    }
}

impl WidgetRef for &ChatWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // In the hybrid inline viewport mode we only draw the interactive
        // bottom pane; history entries are injected directly into scrollback
        // via `Terminal::insert_before`.
        (&self.bottom_pane).render(area, buf);
    }
}

fn add_token_usage(current_usage: &TokenUsage, new_usage: &TokenUsage) -> TokenUsage {
    let cached_input_tokens = match (
        current_usage.cached_input_tokens,
        new_usage.cached_input_tokens,
    ) {
        (Some(current), Some(new)) => Some(current + new),
        (Some(current), None) => Some(current),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    };
    let reasoning_output_tokens = match (
        current_usage.reasoning_output_tokens,
        new_usage.reasoning_output_tokens,
    ) {
        (Some(current), Some(new)) => Some(current + new),
        (Some(current), None) => Some(current),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    };
    TokenUsage {
        input_tokens: current_usage.input_tokens + new_usage.input_tokens,
        cached_input_tokens,
        output_tokens: current_usage.output_tokens + new_usage.output_tokens,
        reasoning_output_tokens,
        total_tokens: current_usage.total_tokens + new_usage.total_tokens,
    }
}
