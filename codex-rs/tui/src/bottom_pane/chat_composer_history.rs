use std::collections::HashMap;
use std::sync::mpsc::Sender;

use tui_textarea::CursorMove;
use tui_textarea::TextArea;

use crate::app_event::AppEvent;
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
    }

    /// Record a message submitted by the user in the current session so it can
    /// be recalled later.
    pub fn record_local_submission(&mut self, text: &str) {
        if !text.is_empty() {
            self.local_history.push(text.to_string());
            self.history_cursor = None;
            self.last_history_text = None;
        }
    }

    /// Should Up/Down key presses be interpreted as history navigation given
    /// the current content and cursor position of `textarea`?
    pub fn should_handle_navigation(&self, textarea: &TextArea) -> bool {
        if self.history_entry_count == 0 && self.local_history.is_empty() {
            return false;
        }

        let lines = textarea.lines();
        if lines.len() == 1 && lines[0].is_empty() {
            return true;
        }

        // Textarea is not empty – only navigate when cursor is at start and
        // text matches last recalled history entry so regular editing is not
        // hijacked.
        let (row, col) = textarea.cursor();
        if row != 0 || col != 0 {
            return false;
        }

        matches!(&self.last_history_text, Some(prev) if prev == &lines.join("\n"))
    }

    /// Handle <Up>. Returns true when the key was consumed and the caller
    /// should request a redraw.
    pub fn navigate_up(
        &mut self,
        textarea: &mut TextArea,
        app_event_tx: &Sender<AppEvent>,
    ) -> bool {
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
        app_event_tx: &Sender<AppEvent>,
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
                Self::replace_textarea_content(textarea, "");
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
    ) {
        if self.history_log_id != Some(log_id) {
            return;
        }
        let Some(text) = entry else { return };
        self.fetched_history.insert(offset, text.clone());

        if self.history_cursor == Some(offset as isize) {
            Self::replace_textarea_content(textarea, &text);
        }
    }

    // ---------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------

    fn populate_history_at_index(
        &mut self,
        global_idx: usize,
        textarea: &mut TextArea,
        app_event_tx: &Sender<AppEvent>,
    ) {
        if global_idx >= self.history_entry_count {
            // Local entry.
            if let Some(text) = self
                .local_history
                .get(global_idx - self.history_entry_count)
            {
                Self::replace_textarea_content(textarea, text);
            }
        } else if let Some(text) = self.fetched_history.get(&global_idx) {
            Self::replace_textarea_content(textarea, text);
        } else if let Some(log_id) = self.history_log_id {
            let op = Op::GetHistoryEntryRequest {
                offset: global_idx,
                log_id,
            };
            let _ = app_event_tx.send(AppEvent::CodexOp(op));
        }
    }

    fn replace_textarea_content(textarea: &mut TextArea, text: &str) {
        textarea.select_all();
        textarea.cut();
        let _ = textarea.insert_str(text);
        textarea.move_cursor(CursorMove::Jump(0, 0));
    }
}
