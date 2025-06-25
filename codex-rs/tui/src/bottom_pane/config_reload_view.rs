use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::prelude::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use super::{BottomPane, BottomPaneView};

/// BottomPane view displaying the diff and prompting to apply or ignore.
pub(crate) struct ConfigReloadView {
    diff: String,
    app_event_tx: AppEventSender,
    done: bool,
}

impl ConfigReloadView {
    /// Create a new view with the unified diff of config changes.
    pub fn new(diff: String, app_event_tx: AppEventSender) -> Self {
        Self { diff, app_event_tx, done: false }
    }
}

impl<'a> BottomPaneView<'a> for ConfigReloadView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Enter => {
                self.app_event_tx.send(AppEvent::ConfigReloadApply);
                self.done = true;
            }
            KeyCode::Esc => {
                self.app_event_tx.send(AppEvent::ConfigReloadIgnore);
                self.done = true;
            }
            _ => {}
        }
        pane.request_redraw();
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn calculate_required_height(&self, area: &Rect) -> u16 {
        area.height
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Config changed (Enter=Apply Esc=Ignore)");
        Paragraph::new(self.diff.clone()).block(block).render(area, buf);
    }

    fn should_hide_when_task_is_done(&mut self) -> bool {
        true
    }
}
