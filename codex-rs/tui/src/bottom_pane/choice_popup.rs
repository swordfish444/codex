use codex_core::protocol_config_types::ReasoningEffort as ReasoningEffortConfig;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;
use strum::IntoEnumIterator;

/// Payload associated with a selected item in a generic choice popup.
pub(crate) enum ChoicePayload {
    ReasoningEffort(ReasoningEffortConfig),
}

pub(crate) struct ChoiceItem {
    pub name: String,
    pub is_current: bool,
    pub description: Option<String>,
    pub payload: ChoicePayload,
}

/// A simple reusable choice popup that displays a fixed list of items and
/// allows the user to select one using Up/Down/Enter.
pub(crate) struct ChoicePopup {
    items: Vec<ChoiceItem>,
    state: ScrollState,
}

impl ChoicePopup {
    pub(crate) fn new_reasoning_effort(current: ReasoningEffortConfig) -> Self {
        let items: Vec<ChoiceItem> = ReasoningEffortConfig::iter()
            .map(|v| ChoiceItem {
                name: v.to_string(),
                is_current: v == current,
                description: None,
                payload: ChoicePayload::ReasoningEffort(v),
            })
            .collect();

        let mut state = ScrollState::new();
        // Default selection to the current value when present
        if let Some((idx, _)) = items.iter().enumerate().find(|(_, it)| it.is_current) {
            state.selected_idx = Some(idx);
        }

        Self { items, state }
    }

    pub(crate) fn move_up(&mut self) {
        let len = self.items.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn move_down(&mut self) {
        let len = self.items.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn selected_payload(&self) -> Option<&ChoicePayload> {
        self.state
            .selected_idx
            .and_then(|idx| self.items.get(idx))
            .map(|it| &it.payload)
    }

    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.items.len().clamp(1, MAX_POPUP_ROWS) as u16
    }
}

impl WidgetRef for &ChoicePopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all: Vec<GenericDisplayRow> = if self.items.is_empty() {
            Vec::new()
        } else {
            self.items
                .iter()
                .map(|item| GenericDisplayRow {
                    name: item.name.clone(),
                    match_indices: None,
                    is_current: item.is_current,
                    description: item.description.clone(),
                })
                .collect()
        };

        render_rows(area, buf, &rows_all, &self.state, MAX_POPUP_ROWS);
    }
}
