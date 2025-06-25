//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.

use bottom_pane_view::BottomPaneView;
use bottom_pane_view::ConditionalUpdate;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::ApprovalRequest;

mod approval_modal_view;
mod mount_view;
mod shell_command_view;
mod bottom_pane_view;
mod chat_composer;
mod chat_composer_history;
mod command_popup;
mod status_indicator_view;

pub(crate) use chat_composer::ChatComposer;
pub(crate) use chat_composer::InputResult;

use approval_modal_view::ApprovalModalView;
use mount_view::{MountAddView, MountRemoveView};
use shell_command_view::ShellCommandView;
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
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) has_input_focus: bool,
    /// Maximum number of visible lines in the chat input composer.
    pub(crate) composer_max_rows: usize,
}

impl BottomPane<'_> {
    pub fn new(params: BottomPaneParams) -> Self {
        Self {
            composer: ChatComposer::new(
                params.has_input_focus,
                params.app_event_tx.clone(),
                params.composer_max_rows,
            ),
            active_view: None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        if let Some(mut view) = self.active_view.take() {
            // During task execution, allow input to pass through status indicator overlay
            if self.is_task_running && view.should_hide_when_task_is_done() {
                // restore overlay view and forward input to composer
                self.active_view = Some(view);
                let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
                if needs_redraw {
                    self.request_redraw();
                }
                return input_result;
            }
            view.handle_key_event(self, key_event);
            if !view.is_complete() {
                self.active_view = Some(view);
            } else if self.is_task_running {
                let height = self.composer.calculate_required_height(&Rect::default());
                self.active_view = Some(Box::new(StatusIndicatorView::new(
                    self.app_event_tx.clone(),
                    height,
                )));
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

    /// Update the status indicator text (only when the `StatusIndicatorView` is
    /// active).
    pub(crate) fn update_status_text(&mut self, text: String) {
        if let Some(view) = &mut self.active_view {
            match view.update_status_text(text) {
                ConditionalUpdate::NeedsRedraw => {
                    self.request_redraw();
                }
                ConditionalUpdate::NoRedraw => {
                    // No redraw needed.
                }
            }
        }
    }

    /// Update the UI to reflect whether this `BottomPane` has input focus.
    pub(crate) fn set_input_focus(&mut self, has_focus: bool) {
        self.has_input_focus = has_focus;
        self.composer.set_input_focus(has_focus);
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;

        match (running, self.active_view.is_some()) {
            (true, false) => {
                // Show status indicator overlay.
                let height = self.composer.calculate_required_height(&Rect::default());
                self.active_view = Some(Box::new(StatusIndicatorView::new(
                    self.app_event_tx.clone(),
                    height,
                )));
                self.request_redraw();
            }
            (false, true) => {
                if let Some(mut view) = self.active_view.take() {
                    if view.should_hide_when_task_is_done() {
                        // Leave self.active_view as None.
                        self.request_redraw();
                    } else {
                        // Preserve the view.
                        self.active_view = Some(view);
                    }
                }
            }
            _ => {
                // No change.
            }
        }
    }

    /// Update the context-left percentage displayed in the composer.
    pub fn set_context_percent(&mut self, pct: f64) {
        self.composer.set_context_left(pct);
    }

    /// Launch interactive mount-add dialog (host, container, [mode]).
    pub fn push_mount_add_interactive(&mut self) {
        let view = MountAddView::new(self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.request_redraw();
    }

    /// Launch interactive mount-remove dialog (container path).
    pub fn push_mount_remove_interactive(&mut self) {
        let view = MountRemoveView::new(self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.request_redraw();
    }

    /// Launch interactive shell-command dialog (prompt for arbitrary command).
    pub fn push_shell_command_interactive(&mut self) {
        let view = ShellCommandView::new(self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
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
        self.request_redraw()
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub fn calculate_required_height(&self, area: &Rect) -> u16 {
        if let Some(view) = &self.active_view {
            view.calculate_required_height(area)
        } else {
            self.composer.calculate_required_height(area)
        }
    }

    pub(crate) fn request_redraw(&self) {
        self.app_event_tx.send(AppEvent::Redraw)
    }

    /// Returns true when the slash-command popup inside the composer is visible.
    pub(crate) fn is_command_popup_visible(&self) -> bool {
        self.active_view.is_none() && self.composer.is_command_popup_visible()
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
}

impl WidgetRef for &BottomPane<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Always render composer, then overlay any active view (e.g., status indicator or modal)
        (&self.composer).render_ref(area, buf);
        if let Some(ov) = &self.active_view {
            ov.render(area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// Construct a BottomPane with default parameters for testing.
    fn make_pane() -> BottomPane<'static> {
        let (tx, _rx) = std::sync::mpsc::channel();
        let app_event_tx = AppEventSender::new(tx);
        BottomPane::new(BottomPaneParams {
            app_event_tx,
            has_input_focus: true,
            composer_max_rows: 3,
        })
    }

    #[test]
    fn forward_input_during_status_indicator() {
        let mut pane = make_pane();
        // Start task to show status indicator overlay
        pane.set_task_running(true);
        // Simulate typing 'h'
        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let result = pane.handle_key_event(key);
        // No submission event is returned
        assert!(matches!(result, InputResult::None));
        // Composer should have recorded the input
        let content = pane.composer.get_input_text();
        assert_eq!(content, "h");
        // Status indicator overlay remains active
        assert!(pane.active_view.is_some());
    }

    #[test]
    fn remove_status_indicator_after_task_complete() {
        let mut pane = make_pane();
        pane.set_task_running(true);
        assert!(pane.active_view.is_some());
        pane.set_task_running(false);
        // Overlay should be removed when task finishes
        assert!(pane.active_view.is_none());
    }
}
