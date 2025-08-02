use std::collections::HashMap;

use tui_textarea::CursorMove;
use tui_textarea::TextArea;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use codex_core::protocol::Op;

/// State machine that manages shell-style history navigation (Up/Down) inside
/// the chat composer. This struct is intentionally decoupled from the
/// rendering widget so the logic remains isolated and easier to test.
pub(crate) struct ChatComposerHistory {
    /// Identifier of the history log as reported by `SessionConfiguredEvent`.
    history_log_id: Option<u64>,
    /// Number of entries already present in the persistent cross-session
    /// history file when the session started.
    history_entry_count: usize,

    /// Messages submitted by the user *during this UI session* (newest at END).
    local_history: Vec<String>,

    /// Cache of persistent history entries fetched on-demand.
    fetched_history: HashMap<usize, String>,

    /// Current cursor within the combined (persistent + local) history. `None`
    /// indicates the user is *not* currently browsing history.
    history_cursor: Option<isize>,

    /// The text that was last inserted into the composer as a result of
    /// history navigation. Used to decide if further Up/Down presses should be
    /// treated as navigation versus normal cursor movement.
    last_history_text: Option<String>,

    /// Search state (active only during Ctrl+R history search).
    search: Option<SearchState>,
}

impl ChatComposerHistory {
    pub fn new() -> Self {
        Self {
            history_log_id: None,
            history_entry_count: 0,
            local_history: Vec::new(),
            fetched_history: HashMap::new(),
            history_cursor: None,
            last_history_text: None,
            search: None,
        }
    }

    /// Update metadata when a new session is configured.
    pub fn set_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history_log_id = Some(log_id);
        self.history_entry_count = entry_count;
        self.fetched_history.clear();
        self.local_history.clear();
        self.history_cursor = None;
        self.last_history_text = None;
        self.search = None;
    }

    /// Expose the current search query when active.
    pub fn search_query(&self) -> Option<&str> {
        self.search.as_ref().map(|s| s.query.as_str())
    }

    /// Returns true when search mode is active and there are matches.
    pub fn search_has_matches(&self) -> bool {
        matches!(self.search.as_ref(), Some(s) if !s.matches.is_empty())
    }

    /// Proactively prefetch the most recent `max_count` persistent history entries for search.
    pub fn prefetch_recent_for_search(&mut self, app_event_tx: &AppEventSender, max_count: usize) {
        let Some(log_id) = self.history_log_id else {
            return;
        };
        if self.history_entry_count == 0 || max_count == 0 {
            return;
        }
        // Start from newest offset and walk backwards
        let mut remaining = max_count;
        let mut offset = self.history_entry_count.saturating_sub(1);
        loop {
            if !self.fetched_history.contains_key(&offset) {
                let op = Op::GetHistoryEntryRequest { offset, log_id };
                app_event_tx.send(AppEvent::CodexOp(op));
                // Do not insert into fetched cache yet; wait for response
            }
            if remaining == 1 || offset == 0 {
                break;
            }
            remaining -= 1;
            offset -= 1;
        }
    }

    /// When search is active but there are no matches (or we want deeper coverage),
    /// fetch additional older persistent entries beyond those already cached.
    pub fn fetch_more_for_search(&mut self, app_event_tx: &AppEventSender, max_count: usize) {
        let Some(log_id) = self.history_log_id else {
            return;
        };
        if self.history_entry_count == 0 || max_count == 0 {
            return;
        }

        // Determine the next range of older offsets to fetch. Start from one before the
        // oldest cached offset; if nothing is cached, start from newest.
        let start_offset = match self.fetched_history.keys().min().copied() {
            Some(min_cached) if min_cached > 0 => min_cached - 1,
            Some(_) => return, // already at oldest
            None => self.history_entry_count.saturating_sub(1),
        };

        let mut remaining = max_count;
        let mut offset = start_offset;
        loop {
            if !self.fetched_history.contains_key(&offset) {
                let op = Op::GetHistoryEntryRequest { offset, log_id };
                app_event_tx.send(AppEvent::CodexOp(op));
            }
            if remaining == 1 || offset == 0 {
                break;
            }
            remaining -= 1;
            offset -= 1;
        }
    }

    /// Record a message submitted by the user in the current session so it can
    /// be recalled later.
    pub fn record_local_submission(&mut self, text: &str) {
        if !text.is_empty() {
            self.local_history.push(text.to_string());
            self.history_cursor = None;
            self.last_history_text = None;
            // Keep search query, but recompute matches if search is active (so newest appears first for empty query)
            if self.search.is_some() {
                let query = self
                    .search
                    .as_ref()
                    .map(|s| s.query.clone())
                    .unwrap_or_default();
                let (matches, selected) = self.recompute_matches_for_query(&query);
                if let Some(s) = &mut self.search {
                    s.matches = matches;
                    s.selected = selected;
                }
            }
        }
    }

    /// Should Up/Down key presses be interpreted as history navigation given
    /// the current content and cursor position of `textarea`?
    pub fn should_handle_navigation(&self, textarea: &TextArea) -> bool {
        if self.history_entry_count == 0 && self.local_history.is_empty() {
            return false;
        }

        if textarea.is_empty() {
            return true;
        }

        // Textarea is not empty – only navigate when cursor is at start and
        // text matches last recalled history entry so regular editing is not
        // hijacked.
        let (row, col) = textarea.cursor();
        if row != 0 || col != 0 {
            return false;
        }

        let lines = textarea.lines();
        matches!(&self.last_history_text, Some(prev) if prev == &lines.join("\n"))
    }

    /// Handle <Up>. Returns true when the key was consumed and the caller
    /// should request a redraw.
    pub fn navigate_up(&mut self, textarea: &mut TextArea, app_event_tx: &AppEventSender) -> bool {
        let total_entries = self.history_entry_count + self.local_history.len();
        if total_entries == 0 {
            return false;
        }

        let next_idx = match self.history_cursor {
            None => (total_entries as isize) - 1,
            Some(0) => return true, // already at oldest
            Some(idx) => idx - 1,
        };

        self.history_cursor = Some(next_idx);
        self.populate_history_at_index(next_idx as usize, textarea, app_event_tx);
        true
    }

    /// Handle <Down>.
    pub fn navigate_down(
        &mut self,
        textarea: &mut TextArea,
        app_event_tx: &AppEventSender,
    ) -> bool {
        let total_entries = self.history_entry_count + self.local_history.len();
        if total_entries == 0 {
            return false;
        }

        let next_idx_opt = match self.history_cursor {
            None => return false, // not browsing
            Some(idx) if (idx as usize) + 1 >= total_entries => None,
            Some(idx) => Some(idx + 1),
        };

        match next_idx_opt {
            Some(idx) => {
                self.history_cursor = Some(idx);
                self.populate_history_at_index(idx as usize, textarea, app_event_tx);
            }
            None => {
                // Past newest – clear and exit browsing mode.
                self.history_cursor = None;
                self.last_history_text = None;
                self.replace_textarea_content(textarea, "");
            }
        }
        true
    }

    /// Integrate a GetHistoryEntryResponse event.
    pub fn on_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
        textarea: &mut TextArea,
    ) -> bool {
        if self.history_log_id != Some(log_id) {
            return false;
        }
        let Some(text) = entry else { return false };
        self.fetched_history.insert(offset, text.clone());

        if self.history_cursor == Some(offset as isize) {
            self.replace_textarea_content(textarea, &text);
            return true;
        }
        // If we are actively searching, newly fetched items might match the query.
        if self.search.is_some() {
            let query = self
                .search
                .as_ref()
                .map(|s| s.query.clone())
                .unwrap_or_default();
            let prev_len = self.search.as_ref().map(|s| s.matches.len()).unwrap_or(0);
            let (matches, selected) = self.recompute_matches_for_query(&query);
            if let Some(s) = &mut self.search {
                s.matches = matches;
                s.selected = selected;
            }
            let new_len = self.search.as_ref().map(|s| s.matches.len()).unwrap_or(0);
            if new_len != prev_len {
                // If first match changed, update the preview.
                self.apply_selected_to_textarea(textarea);
                return true;
            }
        }
        false
    }

    /// Toggle or begin history search mode (Ctrl+R)
    pub fn search(&mut self) {
        if self.search.is_some() {
            self.search = None;
            return;
        }
        self.search = Some(SearchState::new());
        let query = self
            .search
            .as_ref()
            .map(|s| s.query.clone())
            .unwrap_or_default();
        let (matches, selected) = self.recompute_matches_for_query(&query);
        if let Some(s) = &mut self.search {
            s.matches = matches;
            s.selected = selected;
        }
    }

    pub fn is_search_active(&self) -> bool {
        self.search.is_some()
    }

    pub fn exit_search(&mut self) {
        self.search = None;
    }

    /// Append a character to the search query and update the preview.
    /// used when the user types and we update the search query
    pub fn search_append_char(&mut self, ch: char, textarea: &mut TextArea) {
        self.update_search_query(textarea, |query| query.push(ch));
    }

    /// Remove the last character from the search query and update the preview.
    pub fn search_backspace(&mut self, textarea: &mut TextArea) {
        self.update_search_query(textarea, |query| {
            query.pop();
        });
    }

    /// Clear the entire search query and recompute matches (stays in search mode).
    pub fn search_clear_query(&mut self, textarea: &mut TextArea) {
        if self.search.is_some() {
            let (matches, selected) = self.recompute_matches_for_query("");
            if let Some(s) = &mut self.search {
                s.query.clear();
                s.matches = matches;
                s.selected = selected;
            }
            self.apply_selected_to_textarea(textarea);
        }
    }

    /// Move selection to older match (Up).
    pub fn search_move_up(&mut self, textarea: &mut TextArea) {
        if let Some(s) = &mut self.search {
            if !s.matches.is_empty() && s.selected < s.matches.len() - 1 {
                s.selected += 1;
                self.apply_selected_to_textarea(textarea);
            }
        }
    }

    /// Move selection to newer match (Down).
    pub fn search_move_down(&mut self, textarea: &mut TextArea) {
        if let Some(s) = &mut self.search {
            if !s.matches.is_empty() {
                if s.selected > 0 {
                    s.selected -= 1;
                } else {
                    s.selected = 0;
                }
                self.apply_selected_to_textarea(textarea);
            }
        }
    }

    // ---------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------

    fn populate_history_at_index(
        &mut self,
        global_idx: usize,
        textarea: &mut TextArea,
        app_event_tx: &AppEventSender,
    ) {
        if global_idx >= self.history_entry_count {
            // Local entry.
            if let Some(text) = self
                .local_history
                .get(global_idx - self.history_entry_count)
            {
                let t = text.clone();
                self.replace_textarea_content(textarea, &t);
            }
        } else if let Some(text) = self.fetched_history.get(&global_idx) {
            let t = text.clone();
            self.replace_textarea_content(textarea, &t);
        } else if let Some(log_id) = self.history_log_id {
            let op = Op::GetHistoryEntryRequest {
                offset: global_idx,
                log_id,
            };
            app_event_tx.send(AppEvent::CodexOp(op));
        }
    }

    fn replace_textarea_content(&mut self, textarea: &mut TextArea, text: &str) {
        textarea.select_all();
        textarea.cut();
        let _ = textarea.insert_str(text);
        textarea.move_cursor(CursorMove::Jump(0, 0));
        self.last_history_text = Some(text.to_string());
    }

    /// Compute search matches for a given query over known history (local + fetched).
    /// Uses exact, case-insensitive substring matching; newer entries are preferred.
    fn recompute_matches_for_query(&self, query: &str) -> (Vec<SearchMatch>, usize) {
        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut selected: usize = 0;

        if query.is_empty() {
            // Do not return any matches until at least one character is typed.
            return (matches, selected);
        }

        let q_lower = query.to_lowercase();

        // Local entries (newest at end), compute global index then push if contains
        for (i, t) in self.local_history.iter().enumerate() {
            if t.to_lowercase().contains(&q_lower) {
                let global_idx = self.history_entry_count + i;
                matches.push(SearchMatch { idx: global_idx });
            }
        }
        // Fetched persistent entries
        for (idx, t) in self.fetched_history.iter() {
            if t.to_lowercase().contains(&q_lower) {
                matches.push(SearchMatch { idx: *idx });
            }
        }

        // Sort by recency (newer global index first)
        matches.sort_by(|a, b| b.idx.cmp(&a.idx));

        if matches.is_empty() {
            selected = 0;
        } else if selected >= matches.len() {
            selected = matches.len() - 1;
        }
        (matches, selected)
    }

    /// Apply the currently selected match (if any) into the textarea for preview/execute).
    fn apply_selected_to_textarea(&mut self, textarea: &mut TextArea) {
        let Some(s) = &self.search else { return };
        if s.matches.is_empty() {
            // No matches yet (likely empty query or awaiting fetch); leave composer unchanged.
            return;
        }
        let sel_idx = s.matches[s.selected].idx;
        let query = s.query.clone();
        let _ = s;
        if sel_idx >= self.history_entry_count {
            if let Some(text) = self.local_history.get(sel_idx - self.history_entry_count) {
                let t = text.clone();
                self.replace_textarea_content(textarea, &t);
                self.move_cursor_to_first_match(textarea, &t, &query);
                return;
            }
        } else if let Some(text) = self.fetched_history.get(&sel_idx) {
            let t = text.clone();
            self.replace_textarea_content(textarea, &t);
            self.move_cursor_to_first_match(textarea, &t, &query);
            return;
        }
        // Selected refers to an unfetched persistent entry: we can't preview; clear preview.
        self.replace_textarea_content(textarea, "");
    }

    /// Move the cursor to the beginning of the first case-insensitive match of `query` in `text`.
    fn move_cursor_to_first_match(&self, textarea: &mut TextArea, text: &str, query: &str) {
        if query.is_empty() {
            return;
        }
        let tl = text.to_lowercase();
        let ql = query.to_lowercase();
        if let Some(start) = tl.find(&ql) {
            // Compute row and col (in chars) at byte index `start`
            let mut row: u16 = 0;
            let mut col: u16 = 0;
            let mut count: usize = 0;
            for ch in text.chars() {
                if count == start {
                    break;
                }
                if ch == '\n' {
                    row = row.saturating_add(1);
                    col = 0;
                } else {
                    col = col.saturating_add(1);
                }
                count += ch.len_utf8();
            }
            textarea.move_cursor(CursorMove::Jump(row, col));
        }
    }

    // Extract common logic for updating the search query, recomputing matches, and applying selection.
    fn update_search_query<F>(&mut self, textarea: &mut TextArea, modify: F)
    where
        F: FnOnce(&mut String),
    {
        // Move out the current search state or exit if inactive
        let mut state = match self.search.take() {
            Some(s) => s,
            None => return,
        };
        // Clone and modify the query
        let mut query = state.query.clone();
        modify(&mut query);
        // Recompute matches based on modified query
        let (matches, selected) = self.recompute_matches_for_query(&query);
        // Update the moved-out state
        state.query = query;
        state.matches = matches;
        state.selected = selected;
        // Restore the state and apply selection update
        self.search = Some(state);
        self.apply_selected_to_textarea(textarea);
    }
}

#[derive(Debug, Clone)]
struct SearchMatch {
    idx: usize, // global history index
}

#[derive(Debug, Clone)]
struct SearchState {
    query: String,
    matches: Vec<SearchMatch>,
    selected: usize,
}

impl SearchState {
    fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            selected: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used)]
    use super::*;
    use crate::app_event::AppEvent;
    use codex_core::protocol::Op;
    use std::sync::mpsc::channel;

    #[test]
    fn navigation_with_async_fetch() {
        let (tx, rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx);

        let mut history = ChatComposerHistory::new();
        // Pretend there are 3 persistent entries.
        history.set_metadata(1, 3);

        let mut textarea = TextArea::default();

        // First Up should request offset 2 (latest) and await async data.
        assert!(history.should_handle_navigation(&textarea));
        assert!(history.navigate_up(&mut textarea, &tx));

        // Verify that an AppEvent::CodexOp with the correct GetHistoryEntryRequest was sent.
        let event = rx.try_recv().expect("expected AppEvent to be sent");
        let AppEvent::CodexOp(history_request1) = event else {
            panic!("unexpected event variant");
        };
        assert_eq!(
            Op::GetHistoryEntryRequest {
                log_id: 1,
                offset: 2
            },
            history_request1
        );
        assert_eq!(textarea.lines().join("\n"), ""); // still empty

        // Inject the async response.
        assert!(history.on_entry_response(1, 2, Some("latest".into()), &mut textarea));
        assert_eq!(textarea.lines().join("\n"), "latest");

        // Next Up should move to offset 1.
        assert!(history.navigate_up(&mut textarea, &tx));

        // Verify second CodexOp event for offset 1.
        let event2 = rx.try_recv().expect("expected second event");
        let AppEvent::CodexOp(history_request_2) = event2 else {
            panic!("unexpected event variant");
        };
        assert_eq!(
            Op::GetHistoryEntryRequest {
                log_id: 1,
                offset: 1
            },
            history_request_2
        );

        history.on_entry_response(1, 1, Some("older".into()), &mut textarea);
        assert_eq!(textarea.lines().join("\n"), "older");
    }

    #[test]
    fn search_moves_cursor_to_first_match_ascii() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        // Record a local entry that will be matched.
        history.record_local_submission("hello world");

        // Begin search and type a query that has an exact substring match.
        history.search();
        for ch in ['w', 'o'] {
            history.search_append_char(ch, &mut textarea);
        }

        // Expect the textarea to preview the matched entry and the cursor to
        // be positioned at the first character of the first match (the 'w').
        assert_eq!(textarea.lines().join("\n"), "hello world");
        let (row, col) = textarea.cursor();
        assert_eq!((row, col), (0, 6));
    }

    #[test]
    fn search_moves_cursor_to_first_match_multiline_case_insensitive() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        history.record_local_submission("foo\nBARbaz");

        history.search();
        for ch in ['b', 'a', 'r'] {
            history.search_append_char(ch, &mut textarea);
        }

        // Cursor should point to the 'B' on the second line.
        assert_eq!(textarea.lines().join("\n"), "foo\nBARbaz");
        let (row, col) = textarea.cursor();
        assert_eq!((row, col), (1, 0));
    }

    #[test]
    fn search_moves_cursor_correctly_with_multibyte_chars() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        history.record_local_submission("héllo world");

        history.search();
        for ch in ['w', 'o', 'r', 'l', 'd'] {
            history.search_append_char(ch, &mut textarea);
        }

        // The cursor should be after 6 visible characters: "héllo ".
        assert_eq!(textarea.lines().join("\n"), "héllo world");
        let (row, col) = textarea.cursor();
        assert_eq!((row, col), (0, 6));
    }

    #[test]
    fn search_uses_exact() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        history.record_local_submission("hello world");

        history.search();
        for ch in ['h', 'l', 'd'] {
            // non-contiguous; would be fuzzy, not substring
            history.search_append_char(ch, &mut textarea);
        }

        // No exact substring match for the final query; keep the previous preview
        // (from the intermediate "h" match) instead of clearing.
        assert_eq!(textarea.lines().join("\n"), "hello world");
    }

    #[test]
    fn search_prefers_newer_match_by_recency() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        history.record_local_submission("foo one");
        history.record_local_submission("second foo");

        history.search();
        for ch in ['f', 'o', 'o'] {
            history.search_append_char(ch, &mut textarea);
        }

        // Newer entry containing "foo" should be selected first.
        assert_eq!(textarea.lines().join("\n"), "second foo");
    }

    #[test]
    fn search_is_case_insensitive_and_moves_cursor_to_match_start() {
        let mut history = ChatComposerHistory::new();
        let mut textarea = TextArea::default();

        history.record_local_submission("alpha COUNTRY beta");

        history.search();
        for ch in ['c', 'o', 'u', 'n', 't', 'r', 'y'] {
            history.search_append_char(ch, &mut textarea);
        }

        assert_eq!(textarea.lines().join("\n"), "alpha COUNTRY beta");
        // Cursor should be at start of COUNTRY, which begins at col 6 (after "alpha ")
        let (row, col) = textarea.cursor();
        assert_eq!((row, col), (0, 6));
    }
}
