use super::scroll_state::ScrollState;
use codex_common::fuzzy_match::fuzzy_indices;
use codex_common::fuzzy_match::fuzzy_match;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Constraint;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;

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
        self.state.selected_idx.and_then(|idx| {
            rows.get(idx)
                .map(|DisplayRow::Model { name, .. }| name.clone())
        })
    }

    /// Preferred height (rows) including border.
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.visible_rows().len().clamp(1, MAX_RESULTS) as u16
    }

    /// Compute rows to display applying fuzzy filtering and pinning current model.
    fn visible_rows(&self) -> Vec<DisplayRow> {
        // Build candidate list excluding the current model.
        let mut others: Vec<&str> = self
            .options
            .iter()
            .map(|s| s.as_str())
            .filter(|m| *m != self.current_model)
            .collect();

        // Keep original ordering for non-search.
        if self.query.trim().is_empty() {
            let mut rows: Vec<DisplayRow> = Vec::new();
            // Current model first.
            rows.push(DisplayRow::Model {
                name: self.current_model.clone(),
                match_indices: None,
                is_current: true,
            });
            for name in others.drain(..) {
                rows.push(DisplayRow::Model {
                    name: name.to_string(),
                    match_indices: None,
                    is_current: false,
                });
            }
            return rows;
        }

        // Searching: include current model only if it matches.
        let mut rows: Vec<DisplayRow> = Vec::new();
        if let Some(indices) = fuzzy_indices(&self.current_model, &self.query) {
            rows.push(DisplayRow::Model {
                name: self.current_model.clone(),
                match_indices: Some(indices),
                is_current: true,
            });
        }

        // Fuzzy-match the rest and sort by score, then name, then match tightness.
        let mut matches: Vec<(String, Vec<usize>, i32)> = Vec::new();
        for name in others.into_iter() {
            if let Some((indices, score)) = fuzzy_match(name, &self.query) {
                matches.push((name.to_string(), indices, score));
            }
        }
        matches.sort_by(|(a_name, a_idx, a_score), (b_name, b_idx, b_score)| {
            a_score
                .cmp(b_score)
                .then_with(|| a_name.cmp(b_name))
                .then_with(|| a_idx.len().cmp(&b_idx.len()))
        });

        for (name, indices, _score) in matches.into_iter() {
            if name != self.current_model {
                rows.push(DisplayRow::Model {
                    name,
                    match_indices: Some(indices),
                    is_current: false,
                });
            }
        }

        rows
    }
}

/// Row in the model popup.
enum DisplayRow {
    Model {
        name: String,
        match_indices: Option<Vec<usize>>, // indices to bold (char positions)
        is_current: bool,
    },
}

impl WidgetRef for &ModelSelectionPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all = self.visible_rows();

        let mut rows: Vec<Row> = Vec::new();
        if rows_all.is_empty() {
            rows.push(Row::new(vec![Cell::from(Line::from(Span::styled(
                "no matches",
                Style::default().add_modifier(Modifier::ITALIC | Modifier::DIM),
            )))]));
        } else {
            let max_rows_from_area = area.height as usize;
            let visible_rows = MAX_RESULTS
                .min(rows_all.len())
                .min(max_rows_from_area.max(1));

            // Compute starting index based on scroll state and selection.
            let mut start_idx = self.state.scroll_top.min(rows_all.len().saturating_sub(1));
            if let Some(sel) = self.state.selected_idx {
                if sel < start_idx {
                    start_idx = sel;
                } else if visible_rows > 0 {
                    let bottom = start_idx + visible_rows - 1;
                    if sel > bottom {
                        start_idx = sel + 1 - visible_rows;
                    }
                }
            }

            for (i, row) in rows_all
                .into_iter()
                .enumerate()
                .skip(start_idx)
                .take(visible_rows)
            {
                match row {
                    DisplayRow::Model {
                        name,
                        match_indices,
                        is_current,
                    } => {
                        // Highlight fuzzy indices when present.
                        let mut spans: Vec<Span> = Vec::with_capacity(name.len());
                        if let Some(idxs) = match_indices.as_ref() {
                            let mut idx_iter = idxs.iter().peekable();
                            for (char_idx, ch) in name.chars().enumerate() {
                                let mut style = Style::default();
                                if idx_iter.peek().is_some_and(|next| **next == char_idx) {
                                    idx_iter.next();
                                    style = style.add_modifier(Modifier::BOLD);
                                }
                                spans.push(Span::styled(ch.to_string(), style));
                            }
                        } else {
                            spans.push(Span::raw(name.clone()));
                        }

                        let mut cell = Cell::from(Line::from(spans));
                        if Some(i) == self.state.selected_idx {
                            cell = cell.style(
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            );
                        } else if is_current {
                            cell = cell.style(Style::default().fg(Color::Cyan));
                        }
                        rows.push(Row::new(vec![cell]));
                    }
                }
            }
        }

        let table = Table::new(rows, vec![Constraint::Percentage(100)])
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_type(BorderType::QuadrantOutside)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .widths([Constraint::Percentage(100)]);

        table.render(area, buf);
    }
}
