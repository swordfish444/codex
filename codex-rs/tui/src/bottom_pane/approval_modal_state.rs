use std::sync::mpsc::{SendError, Sender};

use crossterm::event::KeyEvent;
use ratatui::{buffer::Buffer, layout::Rect, widgets::WidgetRef};

use crate::{
    app_event::AppEvent,
    user_approval_widget::{ApprovalRequest, UserApprovalWidget},
};

use super::{BottomPane, OverlayState};

/// Modal overlay asking the user to approve/deny a sequence of requests.
pub(crate) struct ApprovalModalState<'a> {
    current: UserApprovalWidget<'a>,
    queue: Vec<ApprovalRequest>,
    app_event_tx: Sender<AppEvent>,
}

impl<'a> ApprovalModalState<'a> {
    pub fn new(request: ApprovalRequest, app_event_tx: Sender<AppEvent>) -> Self {
        Self {
            current: UserApprovalWidget::new(request, app_event_tx.clone()),
            queue: Vec::new(),
            app_event_tx,
        }
    }

    pub fn enqueue_request(&mut self, req: ApprovalRequest) {
        self.queue.push(req);
    }

    /// Advance to next request if the current one is finished.
    fn maybe_advance(&mut self) {
        if self.current.is_complete() {
            if let Some(req) = self.queue.pop() {
                self.current = UserApprovalWidget::new(req, self.app_event_tx.clone());
            }
        }
    }
}

impl<'a> OverlayState<'a> for ApprovalModalState<'a> {
    fn handle_key_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> Result<(), SendError<AppEvent>> {
        self.current.handle_key_event(key_event)?;
        self.maybe_advance();
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.current.is_complete() && self.queue.is_empty()
    }

    fn required_height(&self, area: &Rect) -> u16 {
        self.current.get_height(area)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        (&self.current).render_ref(area, buf);
    }

    fn push_approval_request(&mut self, req: ApprovalRequest) -> bool {
        self.enqueue_request(req);
        true
    }
}
