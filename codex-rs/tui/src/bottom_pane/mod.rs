//! Bottom pane widget: always shows the multiline text input, and – when
//! active – an *overlay* such as a status indicator or approval-request
//! modal.

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;
use std::sync::mpsc::SendError;
use std::sync::mpsc::Sender;

use crate::app_event::AppEvent;
use crate::user_approval_widget::ApprovalRequest;

mod approval_modal_state;
mod status_indicator_state;
mod text_input_state;

pub(crate) use text_input_state::InputResult;
pub(crate) use text_input_state::TextInputState;

use approval_modal_state::ApprovalModalState;
use status_indicator_state::StatusIndicatorState;

/// Trait implemented by every *overlay* that can be shown on top of the text
/// input.
pub(crate) trait OverlayState<'a> {
    /// Handle a key event while the overlay is active.
    fn handle_key_event(
        &mut self,
        pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> Result<(), SendError<AppEvent>>;

    /// Return `true` once the overlay has finished and should be removed.
    fn is_complete(&self) -> bool {
        false
    }

    /// Height required to render the overlay.
    fn required_height(&self, area: &Rect) -> u16;

    /// Render the overlay – assumes the underlying text-input has already been
    /// drawn.
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Update the status indicator text – default: ignore and return false.
    fn update_status_text(&mut self, _text: String) -> bool {
        false
    }

    /// Called when task status toggles. Default: keep overlay.
    fn on_task_running_changed(&mut self, _running: bool) -> bool {
        true // return true to keep overlay
    }

    /// Try to handle approval request; return true if consumed.
    fn push_approval_request(&mut self, _req: ApprovalRequest) -> bool {
        false
    }
}

/// Everything that is drawn in the lower half of the chat UI.
pub(crate) struct BottomPane<'a> {
    text_input: TextInputState<'a>,
    overlay: Option<Box<dyn OverlayState<'a> + 'a>>,

    app_event_tx: Sender<AppEvent>,

    has_input_focus: bool,
    is_task_running: bool,
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: Sender<AppEvent>,
    pub(crate) has_input_focus: bool,
}

impl BottomPane<'_> {
    pub fn new(params: BottomPaneParams) -> Self {
        Self {
            text_input: TextInputState::new(params.has_input_focus),
            overlay: None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
        }
    }

    /// Forward a key event to the active overlay or to the text-input.
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
    ) -> Result<InputResult, SendError<AppEvent>> {
        if let Some(mut overlay) = self.overlay.take() {
            overlay.handle_key_event(self, key_event)?;
            if !overlay.is_complete() {
                self.overlay = Some(overlay);
            } else if self.is_task_running {
                let height = self.text_input.required_height(&Rect::default());
                self.overlay = Some(Box::new(StatusIndicatorState::new(
                    self.app_event_tx.clone(),
                    height,
                )));
            }
            self.request_redraw()?;
            Ok(InputResult::None)
        } else {
            let (res, needs_redraw) = self.text_input.handle_key_event(key_event);
            if needs_redraw {
                self.request_redraw()?;
            }
            Ok(res)
        }
    }

    /// Update the status indicator text (only when the status overlay is active).
    pub(crate) fn update_status_text(&mut self, text: String) -> Result<(), SendError<AppEvent>> {
        if let Some(ov) = &mut self.overlay {
            if ov.update_status_text(text) {
                self.request_redraw()?;
            }
        }
        Ok(())
    }

    pub(crate) fn set_input_focus(&mut self, has_focus: bool) {
        self.has_input_focus = has_focus;
        self.text_input.set_input_focus(has_focus);
    }

    pub fn set_task_running(&mut self, running: bool) -> Result<(), SendError<AppEvent>> {
        self.is_task_running = running;

        match (running, self.overlay.is_some()) {
            (true, false) => {
                // Show status indicator overlay.
                let height = self.text_input.required_height(&Rect::default());
                self.overlay = Some(Box::new(StatusIndicatorState::new(
                    self.app_event_tx.clone(),
                    height,
                )));
                self.request_redraw()?;
            }
            (false, true) => {
                if let Some(mut ov) = self.overlay.take() {
                    if ov.on_task_running_changed(false) {
                        self.overlay = Some(ov);
                    } else {
                        // overlay closed
                    }
                    self.request_redraw()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(
        &mut self,
        request: ApprovalRequest,
    ) -> Result<(), SendError<AppEvent>> {
        if let Some(ov) = self.overlay.as_mut() {
            if ov.push_approval_request(request.clone()) {
                self.request_redraw()?;
                return Ok(());
            }
        }

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalModalState::new(request, self.app_event_tx.clone());
        self.overlay = Some(Box::new(modal));
        self.request_redraw()
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub fn required_height(&self, area: &Rect) -> u16 {
        if let Some(ov) = &self.overlay {
            ov.required_height(area)
        } else {
            self.text_input.required_height(area)
        }
    }

    pub(crate) fn request_redraw(&self) -> Result<(), SendError<AppEvent>> {
        self.app_event_tx.send(AppEvent::Redraw)
    }
}

impl WidgetRef for &BottomPane<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Show overlay if present.
        if let Some(ov) = &self.overlay {
            ov.render(area, buf);
        } else {
            (&self.text_input).render_ref(area, buf);
        }
    }
}
