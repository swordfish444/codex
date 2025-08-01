//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::ApprovalRequest;
use bottom_pane_view::BottomPaneView;
use codex_core::protocol::TokenUsage;
use codex_file_search::FileMatch;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

mod approval_modal_view;
mod bottom_pane_view;
mod chat_composer;
mod chat_composer_history;
mod command_popup;
mod file_search_popup;
mod status_indicator_view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Ignored,
    Handled,
}

pub(crate) use chat_composer::ChatComposer;
pub(crate) use chat_composer::InputResult;

use approval_modal_view::ApprovalModalView;
use status_indicator_view::StatusIndicatorView;

/// Pane displayed in the lower half of the chat UI.
pub(crate) struct BottomPane<'a> {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer<'a>,

    /// If present, this is displayed instead of the `composer`.
    active_view: Option<Box<dyn BottomPaneView<'a> + 'a>>,

    app_event_tx: AppEventSender,
    has_input_focus: bool,
    is_task_running: bool,
    ctrl_c_quit_hint: bool,

    /// Optional live, multi‑line status/"live cell" rendered directly above
    /// the composer while a task is running. Unlike `active_view`, this does
    /// not replace the composer; it augments it.
    live_status: Option<crate::status_indicator_widget::StatusIndicatorWidget>,

    /// True if the active view is the StatusIndicatorView that replaces the
    /// composer during a running task.
    status_view_active: bool,
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) has_input_focus: bool,
    pub(crate) enhanced_keys_supported: bool,
}

impl BottomPane<'_> {
    pub fn new(params: BottomPaneParams) -> Self {
        let enhanced_keys_supported = params.enhanced_keys_supported;
        Self {
            composer: ChatComposer::new(
                params.has_input_focus,
                params.app_event_tx.clone(),
                enhanced_keys_supported,
            ),
            active_view: None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
            ctrl_c_quit_hint: false,
            live_status: None,
            status_view_active: false,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let live_h = self
            .live_status
            .as_ref()
            .map(|s| s.desired_height(width))
            .unwrap_or(0);

        if let Some(view) = self.active_view.as_ref() {
            live_h.saturating_add(view.desired_height(width))
        } else {
            live_h.saturating_add(self.composer.desired_height())
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        if let Some(mut view) = self.active_view.take() {
            view.handle_key_event(self, key_event);
            if !view.is_complete() {
                self.active_view = Some(view);
            }
            self.request_redraw();
            InputResult::None
        } else {
            let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
            if needs_redraw {
                self.request_redraw();
            }
            input_result
        }
    }

    /// Handle Ctrl-C in the bottom pane. If a modal view is active it gets a
    /// chance to consume the event (e.g. to dismiss itself).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        let mut view = match self.active_view.take() {
            Some(view) => view,
            None => return CancellationEvent::Ignored,
        };

        let event = view.on_ctrl_c(self);
        match event {
            CancellationEvent::Handled => {
                if !view.is_complete() {
                    self.active_view = Some(view);
                }
                self.show_ctrl_c_quit_hint();
            }
            CancellationEvent::Ignored => {
                self.active_view = Some(view);
            }
        }
        event
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if self.active_view.is_none() {
            let needs_redraw = self.composer.handle_paste(pasted);
            if needs_redraw {
                self.request_redraw();
            }
        }
    }

    /// Update the status indicator text.
    pub(crate) fn update_status_text(&mut self, text: String) {
        // If a specialized view can handle status updates (e.g. the
        // StatusIndicatorView that replaces the composer), prefer that.
        if let Some(view) = self.active_view.as_mut() {
            match view.update_status_text(text.clone()) {
                bottom_pane_view::ConditionalUpdate::NeedsRedraw => {
                    self.request_redraw();
                    return;
                }
                bottom_pane_view::ConditionalUpdate::NoRedraw => {}
            }
        } else {
            // No active view – show the status indicator in place of the
            // composer to prevent typing while waiting.
            let mut v = StatusIndicatorView::new(self.app_event_tx.clone());
            v.update_text(text);
            self.active_view = Some(Box::new(v));
            self.status_view_active = true;
            self.request_redraw();
            return;
        }

        // Fallback: if the current active view does not consume status
        // updates, show a live overlay above the composer so the animation
        // continues while the view is visible (e.g. approval modal).
        if self.live_status.is_none() {
            self.live_status = Some(crate::status_indicator_widget::StatusIndicatorWidget::new(
                self.app_event_tx.clone(),
            ));
        }
        if let Some(status) = &mut self.live_status {
            status.update_text(text);
        }
        self.request_redraw();
    }

    pub(crate) fn show_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = true;
        self.composer
            .set_ctrl_c_quit_hint(true, self.has_input_focus);
        self.request_redraw();
    }

    pub(crate) fn clear_ctrl_c_quit_hint(&mut self) {
        if self.ctrl_c_quit_hint {
            self.ctrl_c_quit_hint = false;
            self.composer
                .set_ctrl_c_quit_hint(false, self.has_input_focus);
            self.request_redraw();
        }
    }

    pub(crate) fn ctrl_c_quit_hint_visible(&self) -> bool {
        self.ctrl_c_quit_hint
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;

        if running {
            if self.active_view.is_none() {
                self.active_view = Some(Box::new(StatusIndicatorView::new(
                    self.app_event_tx.clone(),
                )));
                self.status_view_active = true;
            }
            self.request_redraw();
        } else {
            self.live_status = None;
            // Drop the status view when a task completes, but keep other
            // modal views (e.g. approval dialogs).
            if let Some(mut view) = self.active_view.take() {
                if !view.should_hide_when_task_is_done() {
                    self.active_view = Some(view);
                    self.status_view_active = false;
                } else {
                    self.status_view_active = false;
                }
            }
        }
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.composer.is_empty()
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    /// Update the *context-window remaining* indicator in the composer. This
    /// is forwarded directly to the underlying `ChatComposer`.
    pub(crate) fn set_token_usage(
        &mut self,
        token_usage: TokenUsage,
        model_context_window: Option<u64>,
    ) {
        self.composer
            .set_token_usage(token_usage, model_context_window);
        self.request_redraw();
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(&mut self, request: ApprovalRequest) {
        let request = if let Some(view) = self.active_view.as_mut() {
            match view.try_consume_approval_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalModalView::new(request, self.app_event_tx.clone());
        self.active_view = Some(Box::new(modal));
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub(crate) fn request_redraw(&self) {
        self.app_event_tx.send(AppEvent::RequestRedraw)
    }

    // --- History helpers ---

    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.composer.set_history_metadata(log_id, entry_count);
    }

    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) {
        let updated = self
            .composer
            .on_history_entry_response(log_id, offset, entry);

        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.composer.on_file_search_result(query, matches);
        self.request_redraw();
    }

    /// Clear the live status cell (e.g., when the streamed text has been
    /// inserted into history and we no longer need the inline preview).
    pub(crate) fn clear_live_status(&mut self) {
        self.live_status = None;
        self.request_redraw();
    }

    /// Restart the live status animation for the next entry. Prefer taking
    /// over the composer when possible; if another view is active (e.g. a
    /// modal), fall back to using the overlay so animation can continue.
    pub(crate) fn restart_live_status_with_text(&mut self, text: String) {
        // Try to restart in the active view (if it's the status view).
        let mut handled = false;
        if let Some(mut view) = self.active_view.take() {
            if view.restart_live_status_with_text(self, text.clone()) {
                handled = true;
            }
            self.status_view_active = true;
            self.active_view = Some(view);
        } else {
            // No view – create a fresh status view which replaces the composer.
            let mut v = StatusIndicatorView::new(self.app_event_tx.clone());
            v.restart_with_text(text);
            self.active_view = Some(Box::new(v));
            self.status_view_active = true;
            self.request_redraw();
            return;
        }
        if handled {
            self.request_redraw();
            return;
        }

        // Fallback: show a fresh overlay widget if another view is active.
        self.live_status = Some(crate::status_indicator_widget::StatusIndicatorWidget::new(
            self.app_event_tx.clone(),
        ));
        if let Some(status) = &mut self.live_status {
            status.restart_with_text(text);
        }
        self.request_redraw();
    }

    /// Remove the active StatusIndicatorView (composer takeover) if present,
    /// restoring the composer for user input.
    pub(crate) fn clear_status_view(&mut self) {
        if self.status_view_active {
            self.active_view = None;
            self.status_view_active = false;
            self.request_redraw();
        }
    }
}

impl WidgetRef for &BottomPane<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let mut y_offset = 0u16;
        if let Some(status) = &self.live_status {
            let live_h = status.desired_height(area.width).min(area.height);
            if live_h > 0 {
                let live_rect = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: live_h,
                };
                status.render_ref(live_rect, buf);
                y_offset = live_h;
            }
        }

        if let Some(ov) = &self.active_view {
            if y_offset < area.height {
                let view_rect = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: area.height - y_offset,
                };
                ov.render(view_rect, buf);
            }
        } else if y_offset < area.height {
            let composer_rect = Rect {
                x: area.x,
                y: area.y + y_offset,
                width: area.width,
                height: area.height - y_offset,
            };
            (&self.composer).render_ref(composer_rect, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use std::path::PathBuf;
    use std::sync::mpsc::channel;

    fn exec_request() -> ApprovalRequest {
        ApprovalRequest::Exec {
            id: "1".to_string(),
            command: vec!["echo".into(), "ok".into()],
            cwd: PathBuf::from("."),
            reason: None,
        }
    }

    #[test]
    fn ctrl_c_on_modal_consumes_and_shows_quit_hint() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
        });
        pane.push_approval_request(exec_request());
        assert_eq!(CancellationEvent::Handled, pane.on_ctrl_c());
        assert!(pane.ctrl_c_quit_hint_visible());
        assert_eq!(CancellationEvent::Ignored, pane.on_ctrl_c());
    }
}
