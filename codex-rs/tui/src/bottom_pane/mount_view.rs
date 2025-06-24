use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::text::Line;
use tui_input::Input;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use super::BottomPane;
use super::BottomPaneView;

/// Interactive view prompting for dynamic mount-add (host/container/mode).
enum MountAddStage {
    Host,
    Container,
    Mode,
}

pub(crate) struct MountAddView<'a> {
    stage: MountAddStage,
    host_input: Input,
    container_input: Input,
    mode_input: Input,
    app_event_tx: AppEventSender,
    done: bool,
}

impl MountAddView<'_> {
    pub fn new(app_event_tx: AppEventSender) -> Self {
        Self {
            stage: MountAddStage::Host,
            host_input: Input::default(),
            container_input: Input::default(),
            mode_input: Input::default(),
            app_event_tx,
            done: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for MountAddView<'a> {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if self.done {
            return;
        }
        match self.stage {
            MountAddStage::Host => {
                if key_event.code == KeyCode::Enter {
                    self.stage = MountAddStage::Container;
                } else {
                    self.host_input.handle_event(&key_event);
                }
            }
            MountAddStage::Container => {
                if key_event.code == KeyCode::Enter {
                    self.stage = MountAddStage::Mode;
                } else {
                    self.container_input.handle_event(&key_event);
                }
            }
            MountAddStage::Mode => {
                if key_event.code == KeyCode::Enter {
                    let host = std::path::PathBuf::from(self.host_input.value());
                    let container = std::path::PathBuf::from(self.container_input.value());
                    let mode = {
                        let m = self.mode_input.value();
                        if m.is_empty() { "rw".to_string() } else { m }
                    };
                    self.app_event_tx.send(AppEvent::MountAdd { host, container, mode });
                    self.done = true;
                } else {
                    self.mode_input.handle_event(&key_event);
                }
            }
        }
        pane.request_redraw();
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn calculate_required_height(&self, _area: &Rect) -> u16 {
        // Prompt + input + border
        1 + 1 + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (prompt, input) = match self.stage {
            MountAddStage::Host => ("Host path:", self.host_input.value()),
            MountAddStage::Container => ("Container path:", self.container_input.value()),
            MountAddStage::Mode => ("Mode (rw|ro):", self.mode_input.value()),
        };
        let paragraph = Paragraph::new(vec![Line::from(prompt), Line::from(input)])
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
        paragraph.render(area, buf);
    }
}

/// Interactive view prompting for dynamic mount-remove (container path).
pub(crate) struct MountRemoveView<'a> {
    container_input: Input,
    app_event_tx: AppEventSender,
    done: bool,
}

impl MountRemoveView<'_> {
    pub fn new(app_event_tx: AppEventSender) -> Self {
        Self {
            container_input: Input::default(),
            app_event_tx,
            done: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for MountRemoveView<'a> {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if self.done {
            return;
        }
        if key_event.code == KeyCode::Enter {
            let container = std::path::PathBuf::from(self.container_input.value());
            self.app_event_tx.send(AppEvent::MountRemove { container });
            self.done = true;
        } else {
            self.container_input.handle_event(&key_event);
        }
        pane.request_redraw();
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn calculate_required_height(&self, _area: &Rect) -> u16 {
        1 + 1 + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(vec![
            Line::from("Container path to unmount:"),
            Line::from(self.container_input.value()),
        ])
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
        paragraph.render(area, buf);
    }
}