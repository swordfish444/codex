use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::ChatWidget;
use crate::file_search::FileSearchManager;
use crate::transcript_app::TranscriptApp;
use crate::tui;
use crate::tui::TuiEvent;
use codex_core::ConversationManager;
use codex_core::config::Config;
use codex_core::protocol::TokenUsage;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::supports_keyboard_enhancement;
use ratatui::layout::Rect;
use ratatui::text::Line;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc::unbounded_channel;

pub(crate) struct App {
    server: Arc<ConversationManager>,
    app_event_tx: AppEventSender,
    chat_widget: ChatWidget,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    config: Config,

    file_search: FileSearchManager,

    transcript_lines: Vec<Line<'static>>,

    // Transcript overlay state
    transcript_overlay: Option<TranscriptApp>,
    // If true, overlay is opened as an Esc-backtrack preview.
    transcript_overlay_is_backtrack: bool,
    deferred_history_lines: Vec<Line<'static>>,
    transcript_saved_viewport: Option<Rect>,

    enhanced_keys_supported: bool,

    /// Controls the animation thread that sends CommitTick events.
    commit_anim_running: Arc<AtomicBool>,

    // Esc-backtracking state
    esc_backtrack_primed: bool,
    esc_backtrack_base: Option<uuid::Uuid>,
    esc_backtrack_count: usize,
}

impl App {
    pub async fn run(
        tui: &mut tui::Tui,
        config: Config,
        initial_prompt: Option<String>,
        initial_images: Vec<PathBuf>,
    ) -> Result<TokenUsage> {
        use tokio_stream::StreamExt;
        let (app_event_tx, mut app_event_rx) = unbounded_channel();
        let app_event_tx = AppEventSender::new(app_event_tx);

        let conversation_manager = Arc::new(ConversationManager::default());

        let enhanced_keys_supported = supports_keyboard_enhancement().unwrap_or(false);

        let chat_widget = ChatWidget::new(
            config.clone(),
            conversation_manager.clone(),
            tui.frame_requester(),
            app_event_tx.clone(),
            initial_prompt,
            initial_images,
            enhanced_keys_supported,
        );

        let file_search = FileSearchManager::new(config.cwd.clone(), app_event_tx.clone());

        let mut app = Self {
            server: conversation_manager,
            app_event_tx,
            chat_widget,
            config,
            file_search,
            enhanced_keys_supported,
            transcript_lines: Vec::new(),
            transcript_overlay: None,
            transcript_overlay_is_backtrack: false,
            deferred_history_lines: Vec::new(),
            transcript_saved_viewport: None,
            commit_anim_running: Arc::new(AtomicBool::new(false)),
            esc_backtrack_primed: false,
            esc_backtrack_base: None,
            esc_backtrack_count: 0,
        };

        let tui_events = tui.event_stream();
        tokio::pin!(tui_events);

        tui.frame_requester().schedule_frame();

        while select! {
            Some(event) = app_event_rx.recv() => {
                app.handle_event(tui, event)?
            }
            Some(event) = tui_events.next() => {
                app.handle_tui_event(tui, event).await?
            }
        } {}
        tui.terminal.clear()?;
        Ok(app.token_usage())
    }

    pub(crate) async fn handle_tui_event(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> Result<bool> {
        if self.transcript_overlay.is_some() {
            // Intercept Esc/Enter when overlay is a backtrack preview.
            let mut handled = false;
            if self.transcript_overlay_is_backtrack {
                match event {
                    TuiEvent::Key(KeyEvent { code: KeyCode::Esc, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. }) => {
                        if self.esc_backtrack_base.is_some() {
                            self.esc_backtrack_count = self.esc_backtrack_count.saturating_add(1);
                            let header_idx =
                                crate::backtrack_helpers::find_nth_last_user_header_index(
                                    &self.transcript_lines,
                                    self.esc_backtrack_count,
                                );
                            let offset = header_idx.map(|idx| {
                                crate::backtrack_helpers::wrapped_offset_before(
                                    &self.transcript_lines,
                                    idx,
                                    tui.terminal.viewport_area.width,
                                )
                            });
                            let hl = crate::backtrack_helpers::highlight_range_for_nth_last_user(
                                &self.transcript_lines,
                                self.esc_backtrack_count,
                            );
                            if let Some(overlay) = &mut self.transcript_overlay {
                                if let Some(off) = offset { overlay.scroll_offset = off; }
                                overlay.set_highlight_range(hl);
                            }
                            tui.frame_requester().schedule_frame();
                            handled = true;
                        }
                    }
                    TuiEvent::Key(KeyEvent { code: KeyCode::Enter, kind: KeyEventKind::Press, .. }) => {
                        // Confirm the backtrack: close overlay, fork, and prefill.
                        let base = self.esc_backtrack_base;
                        let count = self.esc_backtrack_count;
                        self.close_transcript_overlay(tui);
                        if let Some(base_id) = base {
                            if count > 0 {
                                if let Err(e) = self.fork_and_render_backtrack(tui, base_id, count).await {
                                    tracing::error!("Backtrack confirm failed: {e:#}");
                                }
                            }
                        }
                        // Reset backtrack state after confirming.
                        self.esc_backtrack_primed = false;
                        self.esc_backtrack_base = None;
                        self.esc_backtrack_count = 0;
                        handled = true;
                    }
                    _ => {}
                }
            }
            // Forward to overlay if not handled
            if !handled {
                if let Some(overlay) = &mut self.transcript_overlay {
                    overlay.handle_event(tui, event)?;
                    if overlay.is_done {
                        self.close_transcript_overlay(tui);
                        if self.transcript_overlay_is_backtrack {
                            self.esc_backtrack_primed = false;
                            self.esc_backtrack_base = None;
                            self.esc_backtrack_count = 0;
                        }
                    }
                }
            }
            tui.frame_requester().schedule_frame();
        } else {
            match event {
                TuiEvent::Key(key_event) => {
                    self.handle_key_event(tui, key_event).await;
                }
                TuiEvent::Paste(pasted) => {
                    // Many terminals convert newlines to \r when pasting (e.g., iTerm2),
                    // but tui-textarea expects \n. Normalize CR to LF.
                    // [tui-textarea]: https://github.com/rhysd/tui-textarea/blob/4d18622eeac13b309e0ff6a55a46ac6706da68cf/src/textarea.rs#L782-L783
                    // [iTerm2]: https://github.com/gnachman/iTerm2/blob/5d0c0d9f68523cbd0494dad5422998964a2ecd8d/sources/iTermPasteHelper.m#L206-L216
                    let pasted = pasted.replace("\r", "\n");
                    self.chat_widget.handle_paste(pasted);
                }
                TuiEvent::Draw => {
                    tui.draw(
                        self.chat_widget.desired_height(tui.terminal.size()?.width),
                        |frame| {
                            frame.render_widget_ref(&self.chat_widget, frame.area());
                            if let Some((x, y)) = self.chat_widget.cursor_pos(frame.area()) {
                                frame.set_cursor_position((x, y));
                            }
                        },
                    )?;
                }
                #[cfg(unix)]
                TuiEvent::ResumeFromSuspend => {
                    let cursor_pos = tui.terminal.get_cursor_position()?;
                    tui.terminal.set_viewport_area(ratatui::layout::Rect::new(
                        0,
                        cursor_pos.y,
                        0,
                        0,
                    ));
                }
            }
        }
        Ok(true)
    }

    fn handle_event(&mut self, tui: &mut tui::Tui, event: AppEvent) -> Result<bool> {
        match event {
            AppEvent::NewSession => {
                self.chat_widget = ChatWidget::new(
                    self.config.clone(),
                    self.server.clone(),
                    tui.frame_requester(),
                    self.app_event_tx.clone(),
                    None,
                    Vec::new(),
                    self.enhanced_keys_supported,
                );
                tui.frame_requester().schedule_frame();
            }
            AppEvent::InsertHistoryLines(lines) => {
                if let Some(overlay) = &mut self.transcript_overlay {
                    overlay.insert_lines(lines.clone());
                    tui.frame_requester().schedule_frame();
                }
                self.transcript_lines.extend(lines.clone());
                if self.transcript_overlay.is_some() {
                    self.deferred_history_lines.extend(lines);
                } else {
                    tui.insert_history_lines(lines);
                }
            }
            AppEvent::InsertHistoryCell(cell) => {
                if let Some(overlay) = &mut self.transcript_overlay {
                    overlay.insert_lines(cell.transcript_lines());
                    tui.frame_requester().schedule_frame();
                }
                self.transcript_lines.extend(cell.transcript_lines());
                let display = cell.display_lines();
                if !display.is_empty() {
                    if self.transcript_overlay.is_some() {
                        self.deferred_history_lines.extend(display);
                    } else {
                        tui.insert_history_lines(display);
                    }
                }
            }
            AppEvent::StartCommitAnimation => {
                if self
                    .commit_anim_running
                    .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    let tx = self.app_event_tx.clone();
                    let running = self.commit_anim_running.clone();
                    thread::spawn(move || {
                        while running.load(Ordering::Relaxed) {
                            thread::sleep(Duration::from_millis(50));
                            tx.send(AppEvent::CommitTick);
                        }
                    });
                }
            }
            AppEvent::StopCommitAnimation => {
                self.commit_anim_running.store(false, Ordering::Release);
            }
            AppEvent::CommitTick => {
                self.chat_widget.on_commit_tick();
            }
            AppEvent::CodexEvent(event) => {
                self.chat_widget.handle_codex_event(event);
            }
            AppEvent::ExitRequest => {
                return Ok(false);
            }
            AppEvent::CodexOp(op) => self.chat_widget.submit_op(op),
            AppEvent::DiffResult(text) => {
                self.chat_widget.add_diff_output(text);
            }
            AppEvent::StartFileSearch(query) => {
                if !query.is_empty() {
                    self.file_search.on_user_query(query);
                }
            }
            AppEvent::FileSearchResult { query, matches } => {
                self.chat_widget.apply_file_search_result(query, matches);
            }
            AppEvent::UpdateReasoningEffort(effort) => {
                // Keep App-level config in sync with TUI so forks/new sessions inherit overrides.
                self.chat_widget.set_reasoning_effort(effort);
                self.config.model_reasoning_effort = effort;
            }
            AppEvent::UpdateModel(model) => {
                self.chat_widget.set_model(model.clone());
                self.config.model = model;
            }
            AppEvent::UpdateAskForApprovalPolicy(policy) => {
                self.chat_widget.set_approval_policy(policy);
                self.config.approval_policy = policy;
            }
            AppEvent::UpdateSandboxPolicy(policy) => {
                self.chat_widget.set_sandbox_policy(policy.clone());
                self.config.sandbox_policy = policy;
            }
        }
        Ok(true)
    }

    pub(crate) fn token_usage(&self) -> codex_core::protocol::TokenUsage {
        self.chat_widget.token_usage().clone()
    }

    async fn handle_key_event(&mut self, tui: &mut tui::Tui, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.chat_widget.on_ctrl_c();
            }
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } if self.chat_widget.composer_is_empty() => {
                self.app_event_tx.send(AppEvent::ExitRequest);
            }
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                // Enter alternate screen and set viewport to full size.
                let _ = execute!(tui.terminal.backend_mut(), EnterAlternateScreen);
                if let Ok(size) = tui.terminal.size() {
                    self.transcript_saved_viewport = Some(tui.terminal.viewport_area);
                    tui.terminal
                        .set_viewport_area(Rect::new(0, 0, size.width, size.height));
                    let _ = tui.terminal.clear();
                }

                self.transcript_overlay = Some(TranscriptApp::new(self.transcript_lines.clone()));
                tui.frame_requester().schedule_frame();
            }
            KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                // Only handle backtracking when composer is empty to avoid clobbering edits.
                if self.chat_widget.composer_is_empty() {
                    if !self.esc_backtrack_primed {
                        // Arm backtracking and record base conversation.
                        self.esc_backtrack_primed = true;
                        self.esc_backtrack_count = 0;
                        self.esc_backtrack_base = self.chat_widget.session_id();
                    } else if self.transcript_overlay.is_none() {
                        // Open transcript overlay in backtrack preview mode and jump to the target message.
                        self.open_transcript_overlay(tui);
                        self.transcript_overlay_is_backtrack = true;
                        self.esc_backtrack_count = self.esc_backtrack_count.saturating_add(1);
                        let header_idx =
                            crate::backtrack_helpers::find_nth_last_user_header_index(
                                &self.transcript_lines,
                                self.esc_backtrack_count,
                            );
                        let offset = header_idx.map(|idx| {
                            crate::backtrack_helpers::wrapped_offset_before(
                                &self.transcript_lines,
                                idx,
                                tui.terminal.viewport_area.width,
                            )
                        });
                        let hl = crate::backtrack_helpers::highlight_range_for_nth_last_user(
                            &self.transcript_lines,
                            self.esc_backtrack_count,
                        );
                        if let Some(overlay) = &mut self.transcript_overlay {
                            if let Some(off) = offset { overlay.scroll_offset = off; }
                            overlay.set_highlight_range(hl);
                        }
                    } else if self.transcript_overlay_is_backtrack {
                        // Already previewing: step to the next older message.
                        self.esc_backtrack_count = self.esc_backtrack_count.saturating_add(1);
                        let header_idx =
                            crate::backtrack_helpers::find_nth_last_user_header_index(
                                &self.transcript_lines,
                                self.esc_backtrack_count,
                            );
                        let offset = header_idx.map(|idx| {
                            crate::backtrack_helpers::wrapped_offset_before(
                                &self.transcript_lines,
                                idx,
                                tui.terminal.viewport_area.width,
                            )
                        });
                        let hl = crate::backtrack_helpers::highlight_range_for_nth_last_user(
                            &self.transcript_lines,
                            self.esc_backtrack_count,
                        );
                        if let Some(overlay) = &mut self.transcript_overlay {
                            if let Some(off) = offset { overlay.scroll_offset = off; }
                            overlay.set_highlight_range(hl);
                        }
                    }
                }
            }
            // Enter confirms backtrack when primed + count > 0. Otherwise pass to widget.
            KeyEvent { code: KeyCode::Enter, kind: KeyEventKind::Press, .. }
                if self.esc_backtrack_primed && self.esc_backtrack_count > 0 && self.chat_widget.composer_is_empty() =>
            {
                if let Some(base_id) = self.esc_backtrack_base {
                    if let Err(e) = self.fork_and_render_backtrack(tui, base_id, self.esc_backtrack_count).await {
                        tracing::error!("Backtrack confirm failed: {e:#}");
                    }
                }
                // Reset backtrack state after confirming.
                self.esc_backtrack_primed = false;
                self.esc_backtrack_base = None;
                self.esc_backtrack_count = 0;
            }
            KeyEvent {
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.chat_widget.handle_key_event(key_event);
            }
            _ => {
                // Ignore Release key events.
            }
        };
    }

    /// Re-render the full transcript into the terminal scrollback in one call.
    /// Useful when switching sessions to ensure prior history remains visible.
    pub(crate) fn render_transcript_once(&mut self, tui: &mut tui::Tui) {
        if !self.transcript_lines.is_empty() {
            tui.insert_history_lines(self.transcript_lines.clone());
        }
    }

    fn open_transcript_overlay(&mut self, tui: &mut tui::Tui) {
        // Enter alternate screen and set viewport to full size.
        let _ = execute!(tui.terminal.backend_mut(), EnterAlternateScreen);
        if let Ok(size) = tui.terminal.size() {
            self.transcript_saved_viewport = Some(tui.terminal.viewport_area);
            tui.terminal
                .set_viewport_area(Rect::new(0, 0, size.width, size.height));
            let _ = tui.terminal.clear();
        }
        self.transcript_overlay = Some(TranscriptApp::new(self.transcript_lines.clone()));
        tui.frame_requester().schedule_frame();
    }

    fn close_transcript_overlay(&mut self, tui: &mut tui::Tui) {
        // Exit alternate screen and restore viewport.
        let _ = execute!(tui.terminal.backend_mut(), LeaveAlternateScreen);
        if let Some(saved) = self.transcript_saved_viewport.take() {
            tui.terminal.set_viewport_area(saved);
        }
        if !self.deferred_history_lines.is_empty() {
            let lines = std::mem::take(&mut self.deferred_history_lines);
            tui.insert_history_lines(lines);
        }
        self.transcript_overlay = None;
        self.transcript_overlay_is_backtrack = false;
    }

    async fn fork_and_render_backtrack(
        &mut self,
        tui: &mut tui::Tui,
        base_id: uuid::Uuid,
        drop_last_messages: usize,
    ) -> color_eyre::eyre::Result<()> {
        // Compute the text to prefill by extracting the N-th last user message
        // from the UI transcript lines already rendered.
        let prefill = crate::backtrack_helpers::nth_last_user_text(
            &self.transcript_lines,
            drop_last_messages,
        );

        // Fork conversation with the requested drop.
        let fork = self
            .server
            .fork_conversation(base_id, drop_last_messages, self.config.clone())
            .await?;
        // Replace chat widget with one attached to the new conversation.
        self.chat_widget = ChatWidget::new_from_existing(
            self.config.clone(),
            fork.conversation,
            fork.session_configured,
            tui.frame_requester(),
            self.app_event_tx.clone(),
            self.enhanced_keys_supported,
        );

        // Trim transcript to preserve only content up to the selected user message.
        if let Some(cut_idx) = crate::backtrack_helpers::find_nth_last_user_header_index(
            &self.transcript_lines,
            drop_last_messages,
        ) {
            self.transcript_lines.truncate(cut_idx);
        } else {
            self.transcript_lines.clear();
        }
        let _ = tui.terminal.clear();
        self.render_transcript_once(tui);

        // Prefill the composer with the dropped user message text, if any.
        if let Some(text) = prefill {
            if !text.is_empty() {
                self.chat_widget.insert_str(&text);
            }
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    // (moved helper functions to backtrack_helpers.rs)
}
