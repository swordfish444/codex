use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::Clear;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::BottomPane;
use super::BottomPaneView;

/// Simple dropdown to select a model.
pub(crate) struct ModelSelectionView {
    /// Full list of models from provider (deduplicated).
    options: Vec<String>,
    /// Current model pinned at the top of the list.
    current_model: String,
    /// Current zero-based selection index among rendered rows.
    selected_idx: usize,
    /// Query used to filter models via fuzzy match.
    query: String,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

impl ModelSelectionView {
    pub fn new(current_model: &str, app_event_tx: AppEventSender) -> Self {
        // Initially no options; will be populated asynchronously.
        Self {
            options: Vec::new(),
            current_model: current_model.to_string(),
            selected_idx: 0,
            query: String::new(),
            is_complete: false,
            app_event_tx,
        }
    }

    /// Produce the sequence of display rows respecting pinned current model,
    /// sort preference, and search filter.
    fn build_display_rows(&self) -> Vec<DisplayRow> {
        // Determine candidate list excluding the current model (it is always pinned first).
        let others: Vec<&str> = self
            .options
            .iter()
            .map(|s| s.as_str())
            .filter(|m| *m != self.current_model)
            .collect();

        // If not searching, maintain provided ordering; otherwise, we'll score by fuzzy match.
        if self.query.is_empty() {
            let mut rows: Vec<DisplayRow> = Vec::new();
            // Pinned current model always first.
            rows.push(DisplayRow::Model {
                name: self.current_model.clone(),
                match_indices: None,
                is_current: true,
            });
            if !others.is_empty() {
                for name in others {
                    rows.push(DisplayRow::Model {
                        name: name.to_string(),
                        match_indices: None,
                        is_current: false,
                    });
                }
            }
            return rows;
        }

        // Searching: only include current model if it matches the query.
        let mut rows: Vec<DisplayRow> = Vec::new();
        if let Some(indices) = fuzzy_indices(&self.current_model, &self.query) {
            rows.push(DisplayRow::Model {
                name: self.current_model.clone(),
                match_indices: Some(indices),
                is_current: true,
            });
        }

        // Build list of matches among others.
        let mut matches: Vec<(String, Vec<usize>, i32)> = Vec::new();
        for name in others {
            if let Some((indices, score)) = fuzzy_match(name, &self.query) {
                matches.push((name.to_string(), indices, score));
            }
        }
        // Sort by score (ascending => better). If equal, fall back to alphabetical and match tightness.
        matches.sort_by(|(a_name, a_idx, a_score), (b_name, b_idx, b_score)| {
            a_score
                .cmp(b_score)
                .then_with(|| a_name.cmp(b_name))
                .then_with(|| a_idx.len().cmp(&b_idx.len()))
        });

        for (name, indices, _score) in matches {
            // Don't duplicate the current model if it matched above
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

    /// Count how many rows will be rendered (excluding the bottom stats line).
    fn row_count(&self) -> usize {
        let mut count = 0usize;
        if self.query.is_empty() {
            // current model + others
            count += 1; // current
            // Others count (excluding current)
            let others = self
                .options
                .iter()
                .filter(|m| m.as_str() != self.current_model)
                .count();
            count + others
        } else {
            // searching: pinned current + matches
            let mut matches = 1; // current always present
            for name in self
                .options
                .iter()
                .filter(|m| m.as_str() != self.current_model)
            {
                if fuzzy_match(name, &self.query).is_some() {
                    matches += 1;
                }
            }
            matches
        }
    }

    /// Map selected_idx to a selected model name, if any.
    fn selected_model(&self) -> Option<String> {
        if self.row_count() == 0 {
            return None;
        }
        let rows = self.build_display_rows();
        match rows.get(self.selected_idx) {
            Some(DisplayRow::Model { name, .. }) => Some(name.clone()),
            _ => None, // no other placeholder rows are not selectable
        }
    }

    /// Compute the 1-based index of the selected model among all visible models (after filter).
    fn selected_model_position(&self) -> Option<usize> {
        let rows = self.build_display_rows();
        if self.selected_idx >= rows.len() {
            return None;
        }
        let mut pos = 0usize;
        for (i, row) in rows.iter().enumerate() {
            if matches!(row, DisplayRow::Model { .. }) {
                pos += 1;
            }
            if i == self.selected_idx {
                return Some(pos);
            }
        }
        None
    }
}

/// Row that can be rendered in the selector.
enum DisplayRow {
    Model {
        name: String,
        match_indices: Option<Vec<usize>>, // indices to bold (char positions)
        is_current: bool,
    },
}

/// Return indices for a simple case-insensitive subsequence match and a score.
/// Smaller score is better.
fn fuzzy_match(haystack: &str, needle: &str) -> Option<(Vec<usize>, i32)> {
    if needle.is_empty() {
        return Some((Vec::new(), i32::MAX));
    }
    let h_lower = haystack.to_lowercase();
    let n_lower = needle.to_lowercase();
    let mut indices: Vec<usize> = Vec::with_capacity(n_lower.len());
    let mut h_iter = h_lower.char_indices();
    let mut last_pos: Option<usize> = None;

    for ch in n_lower.chars() {
        let mut found = None;
        while let Some((i, hc)) = h_iter.next() {
            if hc == ch {
                found = Some(i);
                break;
            }
        }
        if let Some(pos) = found {
            indices.push(pos);
            last_pos = Some(pos);
        } else {
            return None;
        }
    }

    // Score: window length minus needle length (tighter is better), with a bonus for prefix match.
    let first = *indices.first().unwrap_or(&0);
    let last = last_pos.unwrap_or(first);
    let window = (last as i32 - first as i32 + 1) - (n_lower.len() as i32);
    let mut score = window.max(0);
    if first == 0 {
        score -= 100; // strong bonus for prefix match
    }
    Some((indices, score))
}

fn fuzzy_indices(haystack: &str, needle: &str) -> Option<Vec<usize>> {
    fuzzy_match(haystack, needle).map(|(idx, _)| idx)
}

impl<'a> BottomPaneView<'a> for ModelSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                }
            }
            KeyCode::Down => {
                let max_idx = self.row_count().saturating_sub(1);
                if self.selected_idx < max_idx {
                    self.selected_idx += 1;
                }
            }
            KeyCode::Home => {
                self.selected_idx = 0;
            }
            KeyCode::End => {
                self.selected_idx = self.row_count().saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(model) = self.selected_model() {
                    self.app_event_tx.send(AppEvent::SelectModel(model));
                    self.is_complete = true;
                }
            }
            KeyCode::Esc => {
                if self.query.is_empty() {
                    self.is_complete = true;
                } else {
                    self.query.clear();
                    self.selected_idx = 0; // reset on clear
                }
            }
            KeyCode::Backspace => {
                self.query.pop();
                // After editing, snap to first match if searching; otherwise clamp.
                if self.query.is_empty() {
                    self.selected_idx = self.selected_idx.min(self.row_count().saturating_sub(1));
                } else {
                    self.selected_idx = if self.row_count() > 1 { 1 } else { 0 };
                }
            }
            KeyCode::Char(c) => {
                // Append printable characters to the query.
                if !c.is_control() {
                    self.query.push(c);
                    // When typing, move selection to first match (index 1 because 0 is pinned current).
                    self.selected_idx = if self.row_count() > 1 { 1 } else { 0 };
                }
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the area to prevent ghosting when the list height/width changes between frames.
        Clear.render(area, buf);

        // Compute rows and counts.
        let rows_all = self.build_display_rows();
        let total_rows = rows_all.len();
        let total_models = rows_all
            .iter()
            .filter(|r| matches!(r, DisplayRow::Model { .. }))
            .count();
        let selected_model_pos = self.selected_model_position();

        // Determine content height and rows available for the list (leave one row for stats).
        let content_height = area.height.saturating_sub(2) as usize; // minus borders
        let stats_rows = 1usize; // persistent status line at bottom
        let list_window = content_height.saturating_sub(stats_rows);

        // Mutable self required to adjust scroll_offset; work with a local mutable copy via interior mutability
        // is not necessary; instead compute desired offset and then render using that offset only.
        let mut scroll_offset = 0usize;
        if list_window > 0 {
            // Ensure selected row is visible within [scroll_offset, scroll_offset + list_window)
            if self.selected_idx < scroll_offset {
                scroll_offset = self.selected_idx;
            } else if self.selected_idx >= scroll_offset + list_window {
                scroll_offset = self.selected_idx + 1 - list_window;
            }
            // Clamp to range.
            if total_rows > list_window {
                let max_offset = total_rows - list_window;
                if scroll_offset > max_offset {
                    scroll_offset = max_offset;
                }
            } else {
                scroll_offset = 0;
            }
        } else {
            scroll_offset = 0; // no space for list; still show stats
        }

        // Prepare visible rows slice for list portion.
        let mut visible_rows: Vec<Row> = Vec::new();
        if list_window > 0 {
            let end = (scroll_offset + list_window).min(total_rows);
            for (abs_idx, row) in rows_all.iter().enumerate().take(end).skip(scroll_offset) {
                match row {
                    DisplayRow::Model {
                        name,
                        match_indices,
                        is_current,
                    } => {
                        // Build spans for optional fuzzy highlight.
                        let mut spans: Vec<Span> = Vec::with_capacity(name.len());
                        if let Some(idxs) = match_indices {
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
                        // Selected row style takes precedence.
                        if abs_idx == self.selected_idx {
                            cell = cell.style(
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            );
                        } else if *is_current {
                            // Special color for the current model when not selected.
                            cell = cell.style(Style::default().fg(Color::Cyan));
                        }
                        visible_rows.push(Row::new(vec![cell]));
                    }
                }
            }

            // Fill with blank rows if we have fewer rows than window size, so the stats line stays at bottom.
            while visible_rows.len() < list_window {
                visible_rows.push(Row::new(vec![Cell::from(" ")]));
            }
        }

        // Stats line text: selected position / total models.
        let stats_text = match selected_model_pos {
            Some(pos) => format!(" {pos}/{total_models} models "),
            None => format!(" -/{total_models} models "),
        };
        let mut stats_row = Row::new(vec![Cell::from(stats_text)]);
        stats_row = stats_row.style(Style::default().fg(Color::DarkGray));
        visible_rows.push(stats_row);

        let mut title = String::from(" Select model ");
        if !self.query.is_empty() {
            title.push(' ');
            title.push('(');
            title.push_str(&self.query);
            title.push(')');
        }

        let table = Table::new(
            visible_rows,
            vec![ratatui::prelude::Constraint::Percentage(100)],
        )
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .widths([ratatui::prelude::Constraint::Percentage(100)]);

        table.render(area, buf);
    }

    fn set_model_options(&mut self, current_model: &str, options: Vec<String>) -> bool {
        self.current_model = current_model.to_string();
        // Deduplicate while preserving first occurrence order.
        let mut seen = std::collections::HashSet::new();
        let mut unique: Vec<String> = Vec::with_capacity(options.len());
        for m in options.into_iter() {
            if seen.insert(m.clone()) {
                unique.push(m);
            }
        }
        // Preserve provided ordering without applying preference ranking.
        self.options = unique;

        // Clamp selection to available rows.
        self.selected_idx = self.selected_idx.min(self.row_count().saturating_sub(1));
        true
    }
}
