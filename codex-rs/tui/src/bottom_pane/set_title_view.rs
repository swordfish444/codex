use crossterm::event::{Event as CrosstermEvent, KeyEvent, KeyCode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tui_input::{Input, backend::crossterm::EventHandler};

use super::{BottomPane, BottomPaneView};
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

/// Interactive view prompting for a custom session title.
pub(crate) struct SetTitleView {
    input: Input,
    app_event_tx: AppEventSender,
    done: bool,
}

impl SetTitleView {
    pub fn new(app_event_tx: AppEventSender) -> Self {
        Self {
            input: Input::default(),
            app_event_tx,
            done: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for SetTitleView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if self.done {
            return;
        }
        if key_event.code == KeyCode::Enter {
            let title = self.input.value().to_string();
            self.app_event_tx.send(AppEvent::InlineSetTitle(title));
            self.done = true;
        } else {
            self.input.handle_event(&CrosstermEvent::Key(key_event));
        }
        pane.request_redraw();
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn calculate_required_height(&self, _area: &Rect) -> u16 {
        // prompt + input + border
        1 + 1 + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(vec![
            Line::from("Session title:"),
            Line::from(self.input.value()),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        paragraph.render(area, buf);
    }
}
