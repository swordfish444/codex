use std::num::NonZeroUsize;

use codex_file_search::{self as file_search, FileSearchResults};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Row, Table, WidgetRef, Widget};

/// Maximum number of suggestions shown in the popup.
const MAX_RESULTS: usize = 8;

pub(crate) struct FileSearchPopup {
    /// The query string (text after the `@`).
    query: String,
    /// Cached search results.
    matches: Vec<String>,
    selected_idx: Option<usize>,
}

impl FileSearchPopup {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            selected_idx: None,
        }
    }

    /// Update the popup based on the `query` prefix. If the query changed a new
    /// search is executed (blocking) and the result list refreshed.
    pub(crate) fn update_query(&mut self, query: &str) {
        if query == self.query {
            // No change – nothing to do.
            return;
        }

        self.query.clear();
        self.query.push_str(query);

        // Perform search synchronously – the underlying implementation is
        // reasonably fast for short prefixes and the result count is small
        // (MAX_RESULTS).
        let matches = Self::search_files(query);
        self.matches = matches;

        // Reset selection idx.
        self.selected_idx = if self.matches.is_empty() { None } else { Some(0) };
    }

    /// Preferred height (rows) for the popup including borders.
    pub(crate) fn calculate_required_height(&self, _area: &Rect) -> u16 {
        // For the empty-state we still reserve one row so that the border is
        // rendered with a minimal height (top + bottom lines).
        let rows = self
            .matches
            .len()
            .clamp(1, MAX_RESULTS) as u16;
        rows + 2 /* border */
    }

    fn search_files(prefix: &str) -> Vec<String> {
        use std::path::PathBuf;

        let limit = NonZeroUsize::new(MAX_RESULTS.max(1)).unwrap();
        let threads = NonZeroUsize::new(4).unwrap();

        let search_dir: PathBuf = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Execute the async search on the current runtime.
        use tokio::runtime::{Builder, Handle};
        use tokio::task;

        let fut = file_search::run(prefix, limit, search_dir, Vec::new(), threads);

        let result: anyhow::Result<FileSearchResults> = if let Ok(handle) = Handle::try_current() {
            // Already inside a runtime – run the search in a blocking section.
            task::block_in_place(|| handle.block_on(fut))
        } else {
            // No runtime active; create a lightweight current-thread one.
            match Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt.block_on(fut),
                Err(e) => {
                    tracing::error!("failed to build temporary runtime for file search: {e}");
                    return Vec::new();
                }
            }
        };

        match result {
            Ok(res) => res
                .matches
                .into_iter()
                .map(|(_score, path)| path)
                .collect(),
            Err(err) => {
                tracing::error!("file search failed: {err}");
                Vec::new()
            }
        }
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
            .and_then(|i| self.matches.get(i).map(|s| s.as_str()))
    }
}

impl WidgetRef for FileSearchPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Build table rows – path only.
        let mut rows: Vec<Row> = Vec::new();

        if self.matches.is_empty() {
            rows.push(Row::new(vec![Cell::from("No matches").italic()]));
        } else {
            for (idx, path) in self.matches.iter().take(MAX_RESULTS).enumerate() {
                let mut cell = Cell::from(path.clone());
                if Some(idx) == self.selected_idx {
                    cell = cell.style(Style::default().fg(Color::Black).bg(Color::White));
                }
                rows.push(Row::new(vec![cell]));
            }
        }

        let table = Table::new(rows, &[ratatui::layout::Constraint::Percentage(100)])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(format!("@{query}", query = self.query))
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .column_spacing(1);

        // Consume the table and render it.
        table.render(area, buf);
    }
}
