use super::scroll_state::ScrollState;
use codex_common::fuzzy_match::fuzzy_indices;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

/// Maximum number of options shown in the popup.
const MAX_RESULTS: usize = 8;

/// Visual state for the model-selection popup.
pub(crate) struct ModelSelectionPopup {
    /// The current model (pinned and color-coded when visible).
    current_model: String,
    /// All available model options (deduplicated externally as needed).
    options: Vec<String>,
    /// Current filter query (derived from the composer, e.g. after `/model`).
    query: String,
    /// Selection/scroll state across the visible rows.
    state: ScrollState,
}

impl ModelSelectionPopup {
    pub(crate) fn new(current_model: &str, options: Vec<String>) -> Self {
        let mut this = Self {
            current_model: current_model.to_string(),
            options,
            query: String::new(),
            state: ScrollState::new(),
        };
        // Initialize selection to the first visible row if any, so Enter works immediately.
        let visible_len = this.visible_rows().len();
        this.state.clamp_selection(visible_len);
        this.state
            .ensure_visible(visible_len, MAX_RESULTS.min(visible_len));
        this
    }

    /// Update the current model and option list. Resets/clamps selection as needed.
    pub(crate) fn set_options(&mut self, current_model: &str, options: Vec<String>) {
        self.current_model = current_model.to_string();
        self.options = options;
        let visible_len = self.visible_rows().len();
        self.state.clamp_selection(visible_len);
        self.state
            .ensure_visible(visible_len, MAX_RESULTS.min(visible_len));
    }

    /// Update the fuzzy filter query.
    pub(crate) fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query.clear();
        self.query.push_str(query);
        // Reset/clamp selection based on new filtered list.
        let visible_len = self.visible_rows().len();
        if visible_len == 0 {
            self.state.reset();
        } else {
            self.state.selected_idx = Some(0);
            self.state
                .ensure_visible(visible_len, MAX_RESULTS.min(visible_len));
        }
    }

    /// Move selection cursor up.
    pub(crate) fn move_up(&mut self) {
        let len = self.visible_rows().len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_RESULTS.min(len));
    }

    /// Move selection cursor down.
    pub(crate) fn move_down(&mut self) {
        let len = self.visible_rows().len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, MAX_RESULTS.min(len));
    }

    /// Currently selected model name, if any.
    pub(crate) fn selected_model(&self) -> Option<String> {
        let rows = self.visible_rows();
        self.state
            .selected_idx
            .and_then(|idx| rows.get(idx).map(|r| r.name.clone()))
    }

    /// Preferred height (rows) including border.
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.visible_rows().len().clamp(1, MAX_RESULTS) as u16
    }

    /// Compute rows to display applying fuzzy filtering and pinning current model.
    fn visible_rows(&self) -> Vec<GenericDisplayRow> {
        // Rebuild items on the fly to use the unified selection list behavior.
        use super::selection_list::{SelectionItem, SelectionList};
        let mut items: Vec<SelectionItem<String>> = Vec::new();
        items.push(SelectionItem::new(self.current_model.clone(), self.current_model.clone()).mark_current(true));
        for m in self
            .options
            .iter()
            .filter(|s| *s != &self.current_model)
        {
            items.push(SelectionItem::new(m.clone(), m.clone()));
        }

        let mut list = SelectionList::new(items);
        list.state.selected_idx = self.state.selected_idx;
        list.state.scroll_top = self.state.scroll_top;
        list.set_query(&self.query);
        list.visible_rows().into_iter().map(|(row, _)| row).collect()
    }
}

impl WidgetRef for &ModelSelectionPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all = self.visible_rows();
        render_rows(area, buf, &rows_all, &self.state, MAX_RESULTS);
    }
}
