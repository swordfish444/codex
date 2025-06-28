use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Row, Table, Widget, WidgetRef};
use ratatui::prelude::Constraint;

/// Maximum number of suggestions shown in the popup.
const MAX_RESULTS: usize = 8;

/// Visual state for the file-search popup.
pub(crate) struct FileSearchPopup {
    /// The query string (`@foo` → `foo`).
    query: String,
    /// When `true` the popup is waiting for results to arrive.
    waiting: bool,
    /// Cached matches; paths relative to the search dir.
    matches: Vec<String>,
    /// Currently selected index inside `matches` (if any).
    selected_idx: Option<usize>,
}

impl FileSearchPopup {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            waiting: true,
            matches: Vec::new(),
            selected_idx: None,
        }
    }

    /// Update the query and reset state to *waiting*.
    pub(crate) fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        self.query.clear();
        self.query.push_str(query);

        self.waiting = true;
        self.matches.clear();
        self.selected_idx = None;
    }

    /// Replace matches when a `FileSearchResult` arrives.
    pub(crate) fn set_matches(&mut self, matches: Vec<String>) {
        self.matches = matches;
        self.waiting = false;
        self.selected_idx = if self.matches.is_empty() { None } else { Some(0) };
    }

    /// Move selection cursor up.
    pub(crate) fn move_up(&mut self) {
        if let Some(idx) = self.selected_idx {
            if idx > 0 {
                self.selected_idx = Some(idx - 1);
            }
        }
    }

    /// Move selection cursor down.
    pub(crate) fn move_down(&mut self) {
        if let Some(idx) = self.selected_idx {
            if idx + 1 < self.matches.len() {
                self.selected_idx = Some(idx + 1);
            }
        } else if !self.matches.is_empty() {
            self.selected_idx = Some(0);
        }
    }

    pub(crate) fn selected_match(&self) -> Option<&str> {
        self.selected_idx
            .and_then(|idx| self.matches.get(idx))
            .map(String::as_str)
    }

    /// Preferred height (rows) including border.
    pub(crate) fn calculate_required_height(&self, _area: &Rect) -> u16 {
        // At least 1 row for empty state.
        let rows = if self.waiting {
            1
        } else {
            self.matches.len().clamp(1, MAX_RESULTS)
        } as u16;
        rows + 2 // border
    }
}

impl WidgetRef for &FileSearchPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Prepare rows.
        let rows: Vec<Row> = if self.waiting {
            vec![Row::new(vec![Cell::from(format!(
                " searching for `{}` …",
                self.query
            ))])]
        } else if self.matches.is_empty() {
            vec![Row::new(vec![Cell::from(" no matches ")])]
        } else {
            self.matches
                .iter()
                .take(MAX_RESULTS)
                .enumerate()
                .map(|(i, p)| {
                    let mut cell = Cell::from(p.as_str());
                    if Some(i) == self.selected_idx {
                        cell = cell.style(Style::default().fg(Color::Yellow));
                    }
                    Row::new(vec![cell])
                })
                .collect()
        };

        let table = Table::new(rows, vec![Constraint::Percentage(100)])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(format!(" @{} ", self.query)),
            )
            .widths(&[Constraint::Percentage(100)]);

        table.render(area, buf);
    }
}
