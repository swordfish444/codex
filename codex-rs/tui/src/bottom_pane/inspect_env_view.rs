use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::bottom_pane_view::ConditionalUpdate;
use super::{BottomPane, BottomPaneView};

/// View for displaying the output of `codex inspect-env` in the bottom pane.
pub(crate) struct InspectEnvView {
    lines: Vec<String>,
    done: bool,
}

impl InspectEnvView {
    /// Create a new inspect-env view.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            done: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for InspectEnvView {
    fn update_status_text(&mut self, text: String) -> ConditionalUpdate {
        self.lines.push(text);
        ConditionalUpdate::NeedsRedraw
    }

    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if key_event.code == KeyCode::Enter || key_event.code == KeyCode::Esc {
            self.done = true;
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
            .title("Inspect Env (Enter/Esc to close)");
        let text = self.lines.join("\n");
        Paragraph::new(text).block(block).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn update_status_text_appends_lines() {
        let mut view = InspectEnvView::new();
        assert!(view.lines.is_empty());
        view.update_status_text("foo".to_string());
        view.update_status_text("bar".to_string());
        assert_eq!(view.lines, vec!["foo".to_string(), "bar".to_string()]);
    }

    #[test]
    fn render_includes_lines() {
        let mut view = InspectEnvView::new();
        view.update_status_text("line1".to_string());
        view.update_status_text("line2".to_string());
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 3,
        };
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);
        // Collect all cell symbols into a flat string and verify the lines are present
        let content: String = buf.content().iter().fold(String::new(), |mut acc, cell| {
            acc.push_str(cell.symbol());
            acc
        });
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
    }
}
