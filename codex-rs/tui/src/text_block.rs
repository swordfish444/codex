use crate::cell_widget::CellWidget;
use ratatui::prelude::*;

/// A simple widget that just displays a list of `Line`s via a `Paragraph`.
/// This is the default rendering backend for most `HistoryCell` variants.
/// A simple widget that displays a list of lines via a paragraph.
#[derive(Clone)]
pub struct TextBlock {
    /// The content lines to render.
    pub lines: Vec<Line<'static>>,
}

impl TextBlock {
    /// Create a new text block from preformatted lines.
    pub fn new(lines: Vec<Line<'static>>) -> Self {
        Self { lines }
    }
}

impl CellWidget for TextBlock {
    fn height(&self, width: u16) -> usize {
        // Use the same wrapping configuration as ConversationHistoryWidget so
        // measurement stays in sync with rendering.
        ratatui::widgets::Paragraph::new(self.lines.clone())
            .wrap(crate::conversation_history_widget::wrap_cfg())
            .line_count(width)
    }

    fn render_window(&self, first_visible_line: usize, area: Rect, buf: &mut Buffer) {
        ratatui::widgets::Paragraph::new(self.lines.clone())
            .wrap(crate::conversation_history_widget::wrap_cfg())
            .scroll((first_visible_line as u16, 0))
            .render(area, buf);
    }
}
