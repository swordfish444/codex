use std::sync::mpsc::{SendError, Sender};

use crossterm::event::KeyEvent;
use ratatui::{buffer::Buffer, layout::Rect, widgets::WidgetRef};

use crate::{app_event::AppEvent, status_indicator_widget::StatusIndicatorWidget};

use super::{BottomPane, OverlayState};

pub(crate) struct StatusIndicatorState {
    view: StatusIndicatorWidget,
}

impl StatusIndicatorState {
    pub fn new(app_event_tx: Sender<AppEvent>, height: u16) -> Self {
        Self {
            view: StatusIndicatorWidget::new(app_event_tx, height),
        }
    }

    pub fn update_text(&mut self, text: String) {
        self.view.update_text(text);
    }
}

impl<'a> OverlayState<'a> for StatusIndicatorState {
    fn handle_key_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> Result<(), SendError<AppEvent>> {
        // If underlying view consumes key, schedule redraw.
        if self.view.handle_key_event(key_event)? {
            // we don't have pane reference for redraw; will be done by caller.
        }
        Ok(())
    }

    fn update_status_text(&mut self, text: String) -> bool {
        self.update_text(text);
        true
    }

    fn on_task_running_changed(&mut self, running: bool) -> bool {
        running // keep only while running == true
    }

    fn required_height(&self, _area: &Rect) -> u16 {
        self.view.get_height()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_ref(area, buf);
    }
}
