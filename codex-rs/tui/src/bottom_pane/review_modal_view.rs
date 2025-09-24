use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::key_hint;
use crate::render::line_utils::prefix_lines;

use super::BottomPane;
use super::BottomPaneView;
use super::CancellationEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewModalState {
    Launching,
    Active { url: String },
    Cancelling,
}

pub(crate) struct ReviewModalView {
    state: ReviewModalState,
    cancel_requested: bool,
    app_event_tx: AppEventSender,
}

impl ReviewModalView {
    pub(crate) fn new(state: ReviewModalState, app_event_tx: AppEventSender) -> Self {
        Self {
            state,
            cancel_requested: false,
            app_event_tx,
        }
    }

    pub(crate) fn set_state(&mut self, state: ReviewModalState) {
        if !matches!(state, ReviewModalState::Cancelling) {
            self.cancel_requested = false;
        }
        self.state = state;
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut status_lines = vec![vec!["Web review in progress".bold()].into()];
        match &self.state {
            ReviewModalState::Launching => {
                status_lines.push(vec!["Starting web review server...".into()].into());
            }
            ReviewModalState::Active { url } => {
                status_lines.push(vec!["Open this URL in your browser:".into()].into());
                status_lines.push(vec![url.clone().cyan().underlined()].into());
            }
            ReviewModalState::Cancelling => {
                status_lines.push(vec!["Cancelling web review...".into()].into());
            }
        }
        let prefix = "▌ ".dim();
        lines.extend(prefix_lines(status_lines, prefix.clone(), prefix.clone()));
        lines.push("".into());
        match &self.state {
            ReviewModalState::Cancelling => {
                lines.extend(prefix_lines(
                    vec![vec!["Waiting for the review to stop...".into()].into()],
                    prefix.clone(),
                    prefix,
                ));
            }
            _ => {
                lines.push(vec!["  ".into(), key_hint::ctrl('C'), " cancel review".dim()].into());
            }
        }
        lines
    }
}

impl BottomPaneView for ReviewModalView {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key_event(&mut self, _key_event: KeyEvent) {}

    fn desired_height(&self, width: u16) -> u16 {
        let paragraph = Paragraph::new(self.lines()).wrap(Wrap { trim: false });
        paragraph.line_count(width) as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(self.lines()).wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        if !self.cancel_requested {
            self.app_event_tx.send(AppEvent::ReviewCancelled);
            self.cancel_requested = true;
            self.state = ReviewModalState::Cancelling;
        }
        CancellationEvent::Handled
    }
}
