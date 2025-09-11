use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::app_event_sender::AppEventSender;

use super::BottomPane;
use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::MAX_POPUP_ROWS;
use ratatui::layout::Constraint;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::Row;
use ratatui::widgets::Table;

/// One selectable item in the generic selection list.
pub(crate) type SelectionAction = Box<dyn Fn(&AppEventSender) + Send + Sync>;

pub(crate) struct SelectionItem {
    pub name: String,
    pub description: Option<String>,
    pub is_current: bool,
    pub actions: Vec<SelectionAction>,
}

pub(crate) struct ListSelectionView {
    title: String,
    subtitle: Option<String>,
    footer_hint: Option<String>,
    items: Vec<SelectionItem>,
    state: ScrollState,
    complete: bool,
    app_event_tx: AppEventSender,
}

/// Generic scroll/selection state for a vertical list menu.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ScrollState {
    pub selected_idx: Option<usize>,
    pub scroll_top: usize,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            selected_idx: None,
            scroll_top: 0,
        }
    }

    /// Reset selection and scroll.
    pub fn reset(&mut self) {
        self.selected_idx = None;
        self.scroll_top = 0;
    }

    /// Clamp selection to be within the [0, len-1] range, or None when empty.
    pub fn clamp_selection(&mut self, len: usize) {
        self.selected_idx = match len {
            0 => None,
            _ => Some(self.selected_idx.unwrap_or(0).min(len.saturating_sub(1))),
        };
        if len == 0 {
            self.scroll_top = 0;
        }
    }

    /// Move selection up by one, wrapping to the bottom when necessary.
    pub fn move_up_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => len - 1,
            None => 0,
        });
    }

    /// Move selection down by one, wrapping to the top when necessary.
    pub fn move_down_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        });
    }

    /// Adjust `scroll_top` so that the current `selected_idx` is visible within
    /// the window of `visible_rows`.
    pub fn ensure_visible(&mut self, len: usize, visible_rows: usize) {
        if len == 0 || visible_rows == 0 {
            self.scroll_top = 0;
            return;
        }
        if let Some(sel) = self.selected_idx {
            if sel < self.scroll_top {
                self.scroll_top = sel;
            } else {
                let bottom = self.scroll_top + visible_rows - 1;
                if sel > bottom {
                    self.scroll_top = sel + 1 - visible_rows;
                }
            }
        } else {
            self.scroll_top = 0;
        }
    }
}

/// A generic representation of a display row for selection popups and menus.
pub(crate) struct GenericDisplayRow {
    pub name: String,
    pub match_indices: Option<Vec<usize>>,
    pub is_current: bool,
    pub description: Option<String>,
}

impl GenericDisplayRow {}

/// Render a list of rows using the provided ScrollState, with shared styling
/// and behavior for selection popups and menus.
pub(crate) fn render_rows(
    area: Rect,
    buf: &mut Buffer,
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    _dim_non_selected: bool,
    empty_message: &str,
) {
    let mut rows: Vec<Row> = Vec::new();
    if rows_all.is_empty() {
        rows.push(Row::new(vec![Cell::from(Line::from(
            empty_message.dim().italic(),
        ))]));
    } else {
        let max_rows_from_area = area.height as usize;
        let visible_rows = max_results
            .min(rows_all.len())
            .min(max_rows_from_area.max(1));

        // Compute starting index based on scroll state and selection.
        let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
        if let Some(sel) = state.selected_idx {
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
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(visible_rows)
        {
            let GenericDisplayRow {
                name,
                match_indices,
                is_current: _is_current,
                description,
            } = row;

            // Highlight fuzzy indices when present.
            let mut spans: Vec<Span> = Vec::with_capacity(name.len());
            if let Some(idxs) = match_indices.as_ref() {
                let mut idx_iter = idxs.iter().peekable();
                for (char_idx, ch) in name.chars().enumerate() {
                    if idx_iter.peek().is_some_and(|next| **next == char_idx) {
                        idx_iter.next();
                        spans.push(ch.to_string().bold());
                    } else {
                        spans.push(ch.to_string().into());
                    }
                }
            } else {
                spans.push(name.clone().into());
            }

            if let Some(desc) = description.as_ref() {
                spans.push("  ".into());
                spans.push(desc.clone().dim());
            }

            let mut cell = Cell::from(Line::from(spans));
            if Some(i) == state.selected_idx {
                cell = cell.style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            }
            rows.push(Row::new(vec![cell]));
        }
    }

    let table = Table::new(rows, vec![Constraint::Percentage(100)])
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_type(BorderType::QuadrantOutside)
                .border_style(Style::default().add_modifier(Modifier::DIM)),
        )
        .widths([Constraint::Percentage(100)]);

    table.render(area, buf);
}

impl ListSelectionView {
    fn dim_prefix_span() -> Span<'static> {
        "▌ ".dim()
    }

    fn render_dim_prefix_line(area: Rect, buf: &mut Buffer) {
        let para = Paragraph::new(Line::from(Self::dim_prefix_span()));
        para.render(area, buf);
    }
    pub fn new(
        title: String,
        subtitle: Option<String>,
        footer_hint: Option<String>,
        items: Vec<SelectionItem>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut s = Self {
            title,
            subtitle,
            footer_hint,
            items,
            state: ScrollState::new(),
            complete: false,
            app_event_tx,
        };
        let len = s.items.len();
        if let Some(idx) = s.items.iter().position(|it| it.is_current) {
            s.state.selected_idx = Some(idx);
        }
        s.state.clamp_selection(len);
        s.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
        s
    }

    fn move_up(&mut self) {
        let len = self.items.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    fn move_down(&mut self) {
        let len = self.items.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    fn accept(&mut self) {
        if let Some(idx) = self.state.selected_idx {
            if let Some(item) = self.items.get(idx) {
                for act in &item.actions {
                    act(&self.app_event_tx);
                }
                self.complete = true;
            }
        } else {
            self.complete = true;
        }
    }

    fn cancel(&mut self) {
        // Close the popup without performing any actions.
        self.complete = true;
    }
}

impl BottomPaneView for ListSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => self.cancel(),
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.accept(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let rows = (self.items.len()).clamp(1, MAX_POPUP_ROWS);
        // +1 for the title row, +1 for optional subtitle, +1 for optional footer
        let mut height = rows as u16 + 1;
        if self.subtitle.is_some() {
            // +1 for subtitle, +1 for a blank spacer line beneath it
            height = height.saturating_add(2);
        }
        if self.footer_hint.is_some() {
            height = height.saturating_add(2);
        }
        height
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let title_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };

        let title_spans: Vec<Span<'static>> =
            vec![Self::dim_prefix_span(), self.title.clone().bold()];
        let title_para = Paragraph::new(Line::from(title_spans));
        title_para.render(title_area, buf);

        let mut next_y = area.y.saturating_add(1);
        if let Some(sub) = &self.subtitle {
            let subtitle_area = Rect {
                x: area.x,
                y: next_y,
                width: area.width,
                height: 1,
            };
            let subtitle_spans: Vec<Span<'static>> =
                vec![Self::dim_prefix_span(), sub.clone().dim()];
            let subtitle_para = Paragraph::new(Line::from(subtitle_spans));
            subtitle_para.render(subtitle_area, buf);
            // Render the extra spacer line with the dimmed prefix to align with title/subtitle
            let spacer_area = Rect {
                x: area.x,
                y: next_y.saturating_add(1),
                width: area.width,
                height: 1,
            };
            Self::render_dim_prefix_line(spacer_area, buf);
            next_y = next_y.saturating_add(2);
        }

        let footer_reserved = if self.footer_hint.is_some() { 2 } else { 0 };
        let rows_area = Rect {
            x: area.x,
            y: next_y,
            width: area.width,
            height: area
                .height
                .saturating_sub(next_y.saturating_sub(area.y))
                .saturating_sub(footer_reserved),
        };

        let rows: Vec<GenericDisplayRow> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let is_selected = self.state.selected_idx == Some(i);
                let prefix = if is_selected { '>' } else { ' ' };
                let name_with_marker = if it.is_current {
                    format!("{} (current)", it.name)
                } else {
                    it.name.clone()
                };
                let display_name = format!("{} {}. {}", prefix, i + 1, name_with_marker);
                GenericDisplayRow {
                    name: display_name,
                    match_indices: None,
                    is_current: it.is_current,
                    description: it.description.clone(),
                }
            })
            .collect();
        if rows_area.height > 0 {
            render_rows(
                rows_area,
                buf,
                &rows,
                &self.state,
                MAX_POPUP_ROWS,
                true,
                "no matches",
            );
        }

        if let Some(hint) = &self.footer_hint {
            let footer_area = Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            };
            let footer_para = Paragraph::new(hint.clone().dim());
            footer_para.render(footer_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn wrap_navigation_and_visibility() {
        let mut s = ScrollState::new();
        let len = 10;
        let vis = 5;

        s.clamp_selection(len);
        assert_eq!(s.selected_idx, Some(0));
        s.ensure_visible(len, vis);
        assert_eq!(s.scroll_top, 0);

        s.move_up_wrap(len);
        s.ensure_visible(len, vis);
        assert_eq!(s.selected_idx, Some(len - 1));
        match s.selected_idx {
            Some(sel) => assert!(s.scroll_top <= sel),
            None => panic!("expected Some(selected_idx) after wrap"),
        }

        s.move_down_wrap(len);
        s.ensure_visible(len, vis);
        assert_eq!(s.selected_idx, Some(0));
        assert_eq!(s.scroll_top, 0);
    }
}
