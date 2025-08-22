use crate::app::App;
use crate::backtrack_helpers;
use crate::tui;
use crate::tui::TuiEvent;
use crate::transcript_app::TranscriptApp;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;

impl App {
    // Public entrypoints first (most important)

    /// Route TUI events to the overlay when present, handling backtrack preview
    /// interactions (Esc to step target, Enter to confirm) and overlay lifecycle.
    pub(crate) async fn handle_backtrack_overlay_event(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> Result<bool> {
        // Intercept Esc/Enter when overlay is a backtrack preview.
        let mut handled = false;
        if self.transcript_overlay_is_backtrack {
            match event {
                TuiEvent::Key(KeyEvent { code: KeyCode::Esc, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. }) => {
                    if self.esc_backtrack_base.is_some() {
                        self.esc_backtrack_count = self.esc_backtrack_count.saturating_add(1);
                        let header_idx = backtrack_helpers::find_nth_last_user_header_index(
                            &self.transcript_lines,
                            self.esc_backtrack_count,
                        );
                        let offset = header_idx.map(|idx| backtrack_helpers::wrapped_offset_before(
                            &self.transcript_lines,
                            idx,
                            tui.terminal.viewport_area.width,
                        ));
                        let hl = backtrack_helpers::highlight_range_for_nth_last_user(
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
                    if let Some(base_id) = base
                        && count > 0
                        && let Err(e) = self.fork_and_render_backtrack(tui, base_id, count).await
                    {
                        tracing::error!("Backtrack confirm failed: {e:#}");
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
        if !handled
            && let Some(overlay) = &mut self.transcript_overlay
        {
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
        tui.frame_requester().schedule_frame();
        Ok(true)
    }

    /// Handle global Esc presses for backtracking when no overlay is present.
    pub(crate) fn handle_backtrack_esc_key(&mut self, tui: &mut tui::Tui) {
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
                let header_idx = backtrack_helpers::find_nth_last_user_header_index(
                    &self.transcript_lines,
                    self.esc_backtrack_count,
                );
                let offset = header_idx.map(|idx| backtrack_helpers::wrapped_offset_before(
                    &self.transcript_lines,
                    idx,
                    tui.terminal.viewport_area.width,
                ));
                let hl = backtrack_helpers::highlight_range_for_nth_last_user(
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
                let header_idx = backtrack_helpers::find_nth_last_user_header_index(
                    &self.transcript_lines,
                    self.esc_backtrack_count,
                );
                let offset = header_idx.map(|idx| backtrack_helpers::wrapped_offset_before(
                    &self.transcript_lines,
                    idx,
                    tui.terminal.viewport_area.width,
                ));
                let hl = backtrack_helpers::highlight_range_for_nth_last_user(
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

    /// Fork the conversation and render the trimmed history; prefill composer.
    pub(crate) async fn fork_and_render_backtrack(
        &mut self,
        tui: &mut tui::Tui,
        base_id: uuid::Uuid,
        drop_last_messages: usize,
    ) -> color_eyre::eyre::Result<()> {
        // Compute the text to prefill by extracting the N-th last user message
        // from the UI transcript lines already rendered.
        let prefill = backtrack_helpers::nth_last_user_text(&self.transcript_lines, drop_last_messages);

        // Fork conversation with the requested drop.
        let fork = self
            .server
            .fork_conversation(base_id, drop_last_messages, self.config.clone())
            .await?;
        // Replace chat widget with one attached to the new conversation.
        self.chat_widget = crate::chatwidget::ChatWidget::new_from_existing(
            self.config.clone(),
            fork.conversation,
            fork.session_configured,
            tui.frame_requester(),
            self.app_event_tx.clone(),
            self.enhanced_keys_supported,
        );

        // Trim transcript to preserve only content up to the selected user message.
        if let Some(cut_idx) = backtrack_helpers::find_nth_last_user_header_index(
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
        if let Some(text) = prefill
            && !text.is_empty()
        {
            self.chat_widget.insert_str(&text);
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    // Internal helpers

    pub(crate) fn open_transcript_overlay(&mut self, tui: &mut tui::Tui) {
        // Enter alternate screen and set viewport to full size.
        let _ = execute!(tui.terminal.backend_mut(), EnterAlternateScreen);
        if let Ok(size) = tui.terminal.size() {
            self.transcript_saved_viewport = Some(tui.terminal.viewport_area);
            tui.terminal
                .set_viewport_area(ratatui::layout::Rect::new(0, 0, size.width, size.height));
            let _ = tui.terminal.clear();
        }
        self.transcript_overlay = Some(TranscriptApp::new(self.transcript_lines.clone()));
        tui.frame_requester().schedule_frame();
    }

    pub(crate) fn close_transcript_overlay(&mut self, tui: &mut tui::Tui) {
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

    /// Re-render the full transcript into the terminal scrollback in one call.
    /// Useful when switching sessions to ensure prior history remains visible.
    pub(crate) fn render_transcript_once(&mut self, tui: &mut tui::Tui) {
        if !self.transcript_lines.is_empty() {
            tui.insert_history_lines(self.transcript_lines.clone());
        }
    }
}
