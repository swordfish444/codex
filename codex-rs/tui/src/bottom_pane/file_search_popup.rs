use codex_file_search::FileMatch;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;
use std::fs;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

/// Visual state for the file-search popup.
pub(crate) struct FileSearchPopup {
    /// Query corresponding to the `matches` currently shown.
    display_query: String,
    /// Latest query typed by the user. May differ from `display_query` when
    /// a search is still in-flight.
    pending_query: String,
    /// When `true` we are still waiting for results for `pending_query`.
    waiting: bool,
    /// Cached matches; paths relative to the search dir.
    matches: Vec<FileMatch>,
    /// Shared selection/scroll state.
    state: ScrollState,
}

impl FileSearchPopup {
    pub(crate) fn new() -> Self {
        // If pending_query is empty, pre-populate matches with files in current dir.
        let mut popup = Self {
            display_query: String::new(),
            pending_query: String::new(),
            waiting: true,
            matches: Vec::new(),
            state: ScrollState::new(),
        }
    }

    /// Update the query and reset state to *waiting*.
    pub(crate) fn set_query(&mut self, query: &str) {
        if query == self.pending_query {
            return;
        }

        // Determine if current matches are still relevant.
        let keep_existing = query.starts_with(&self.display_query);

        self.pending_query.clear();
        self.pending_query.push_str(query);

        self.waiting = true; // waiting for new results

        if !keep_existing {
            self.matches.clear();
            self.state.reset();
        }

        // If query is empty, show files in current directory.
        if query.is_empty() {
            self.populate_current_dir_if_empty_query();
            self.waiting = false;
        }
    }

    /// Put the popup into an "idle" state used for an empty query (just "@").
    /// Shows a hint instead of matches until the user types more characters.
    pub(crate) fn set_empty_prompt(&mut self) {
        self.display_query.clear();
        self.pending_query.clear();
        self.waiting = false;
        self.matches.clear();
        // Reset selection/scroll state when showing the empty prompt.
        self.state.reset();
    }

    /// Replace matches when a `FileSearchResult` arrives.
    /// Replace matches. Only applied when `query` matches `pending_query`.
    pub(crate) fn set_matches(&mut self, query: &str, matches: Vec<FileMatch>) {
        if query != self.pending_query {
            return; // stale
        }

        self.display_query = query.to_string();
        self.matches = matches;
        self.waiting = false;
        let len = self.matches.len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    /// Move selection cursor up.
    pub(crate) fn move_up(&mut self) {
        let len = self.matches.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    /// Move selection cursor down.
    pub(crate) fn move_down(&mut self) {
        let len = self.matches.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn selected_match(&self) -> Option<&str> {
        self.state
            .selected_idx
            .and_then(|idx| self.matches.get(idx))
            .map(|file_match| file_match.path.as_str())
    }

    pub(crate) fn calculate_required_height(&self) -> u16 {
        // Row count depends on whether we already have matches. If no matches
        // yet (e.g. initial search or query with no results) reserve a single
        // row so the popup is still visible. When matches are present we show
        // up to MAX_RESULTS regardless of the waiting flag so the list
        // remains stable while a newer search is in-flight.

        self.matches.len().clamp(1, MAX_POPUP_ROWS) as u16
    }

    /// Populate matches with files in the current directory if the query is empty.
    fn populate_current_dir_if_empty_query(&mut self) {
        if !self.pending_query.is_empty() {
            return;
        }
        // Only populate if matches is empty (avoid overwriting search results).
        if !self.matches.is_empty() {
            return;
        }
        let mut entries: Vec<FileMatch> = Vec::new();
        if let Ok(read_dir) = fs::read_dir(".") {
            for entry in read_dir.flatten().take(MAX_RESULTS) {
                if let Ok(file_type) = entry.file_type() {
                    // Skip hidden files (dotfiles) for a cleaner popup.
                    let file_name = entry.file_name();
                    let file_name_str = file_name.to_string_lossy();
                    if file_name_str.starts_with('.') {
                        continue;
                    }
                    // Only show files and directories (not symlinks, etc).
                    if file_type.is_file() || file_type.is_dir() {
                        entries.push(FileMatch {
                            path: file_name_str.to_string(),
                            indices: Some(Vec::new()), // No highlights for empty query.
                            score: 0,
                        });
                    }
                }
            }
        }
        self.matches = entries;
        self.selected_idx = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }
}

impl WidgetRef for &FileSearchPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Convert matches to GenericDisplayRow, translating indices to usize at the UI boundary.
        let rows_all: Vec<GenericDisplayRow> = if self.matches.is_empty() {
            Vec::new()
        } else {
            self.matches
                .iter()
                .map(|m| GenericDisplayRow {
                    name: m.path.clone(),
                    match_indices: m
                        .indices
                        .as_ref()
                        .map(|v| v.iter().map(|&i| i as usize).collect()),
                    is_current: false,
                    description: None,
                })
                .collect()
        };

        if self.waiting && rows_all.is_empty() {
            // Render a minimal waiting stub using the shared renderer (no rows -> "no matches").
            render_rows(area, buf, &[], &self.state, MAX_POPUP_ROWS);
        } else {
            render_rows(area, buf, &rows_all, &self.state, MAX_POPUP_ROWS);
        }
    }
}
