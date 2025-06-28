use std::num::NonZeroUsize;

use codex_file_search::FileSearchResults;
use codex_file_search::{self as file_search};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use std::path::Path;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tokio::task;

/// Maximum number of suggestions shown in the popup.
const MAX_RESULTS: usize = 8;

pub(crate) struct FileSearchPopup {
    /// The query string (text after the `@`).
    query: String,
    search_dir: PathBuf,
    /// Cached search results.
    matches: Vec<String>,
    selected_idx: Option<usize>,
}

impl FileSearchPopup {
    pub(crate) fn new(search_dir: PathBuf) -> Self {
        Self {
            query: String::new(),
            search_dir,
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
        let matches = Self::search_files(query, &self.search_dir);
        self.matches = matches;

        // Reset selection idx.
        self.selected_idx = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Preferred height (rows) for the popup including borders.
    pub(crate) fn calculate_required_height(&self, _area: &Rect) -> u16 {
        // For the empty-state we still reserve one row so that the border is
        // rendered with a minimal height (top + bottom lines).
        let rows = self.matches.len().clamp(1, MAX_RESULTS) as u16;
        rows + 2 /* border */
    }

    fn search_files(prefix: &str, search_dir: &Path) -> Vec<String> {
        #[allow(clippy::unwrap_used)]
        let limit = NonZeroUsize::new(MAX_RESULTS.max(1)).unwrap();
        #[allow(clippy::unwrap_used)]
        let threads = NonZeroUsize::new(4).unwrap();

        // Execute the async search on the current runtime.
        let future = file_search::run(prefix, limit, search_dir, Vec::new(), threads);
        let handle = Handle::current();
        let result: anyhow::Result<FileSearchResults> =
            task::block_in_place(|| handle.block_on(future));

        match result {
            Ok(res) => res.matches.into_iter().map(|(_score, path)| path).collect(),
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
