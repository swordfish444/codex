use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tui_input::{Input, backend::crossterm::EventHandler};

use super::BottomPane;
use super::BottomPaneView;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

/// Interactive view prompting for a shell command to run in the container.
pub(crate) struct ShellCommandView {
    input: Input,
    app_event_tx: AppEventSender,
    done: bool,
}

impl ShellCommandView {
    pub fn new(app_event_tx: AppEventSender) -> Self {
        Self {
            input: Input::default(),
            app_event_tx,
            done: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for ShellCommandView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        // Exit shell prompt on Ctrl+M
        if let KeyEvent {
            code: KeyCode::Char('m'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } = key_event
        {
            self.done = true;
            pane.request_redraw();
            return;
        }
        if self.done {
            return;
        }
        if key_event.code == KeyCode::Enter {
            let cmd = self.input.value().to_string();
            self.app_event_tx.send(AppEvent::ShellCommand(cmd));
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
        // Prompt line + input line + border overhead
        1 + 1 + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(vec![
            ratatui::text::Line::from("Shell command:"),
            ratatui::text::Line::from(self.input.value()),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::bottom_pane::{BottomPane, BottomPaneParams};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::mpsc;

    #[test]
    fn submit_shell_command_emits_event() {
        let (tx, rx) = mpsc::channel();
        let evt_tx = AppEventSender::new(tx);
        let mut view = ShellCommandView::new(evt_tx.clone());
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: evt_tx.clone(),
            has_input_focus: true,
            composer_max_rows: 1,
        });
        // Enter command 'a'
        view.handle_key_event(
            &mut pane,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        view.handle_key_event(&mut pane, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Skip initial redraw event(s)
        let mut event;
        loop {
            event = rx.recv().unwrap();
            if matches!(event, AppEvent::ShellCommand(_)) {
                break;
            }
        }
        if let AppEvent::ShellCommand(cmd) = event {
            assert_eq!(cmd, "a");
        } else {
            panic!("expected ShellCommand event, got {:?}", event);
        }
    }
}
