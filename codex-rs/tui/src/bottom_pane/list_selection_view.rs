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
use textwrap::wrap;

use crate::app_event_sender::AppEventSender;
use crate::render::border::draw_history_border;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::measure_rows_height;
use super::selection_popup_common::render_rows;

/// One selectable item in the generic selection list.
pub(crate) type SelectionAction = Box<dyn Fn(&AppEventSender) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeaderLine {
    Text { text: String, italic: bool },
    Command { command: String },
    Spacer,
}

pub(crate) struct SelectionItem {
    pub name: String,
    pub description: Option<String>,
    pub is_current: bool,
    pub actions: Vec<SelectionAction>,
    pub dismiss_on_select: bool,
    pub search_value: Option<String>,
}

#[derive(Default)]
pub(crate) struct SelectionViewParams {
    pub title: String,
    pub subtitle: Option<String>,
    pub footer_hint: Option<String>,
    pub items: Vec<SelectionItem>,
    pub is_searchable: bool,
    pub search_placeholder: Option<String>,
    pub header: Vec<HeaderLine>,
}

pub(crate) struct ListSelectionView {
    title: String,
    subtitle: Option<String>,
    footer_hint: Option<String>,
    items: Vec<SelectionItem>,
    state: ScrollState,
    complete: bool,
    app_event_tx: AppEventSender,
    is_searchable: bool,
    search_query: String,
    search_placeholder: Option<String>,
    filtered_indices: Vec<usize>,
    last_selected_actual_idx: Option<usize>,
    header: Vec<HeaderLine>,
}

impl ListSelectionView {
    pub fn new(params: SelectionViewParams, app_event_tx: AppEventSender) -> Self {
        let mut s = Self {
            title: params.title,
            subtitle: params.subtitle,
            footer_hint: params.footer_hint,
            items: params.items,
            state: ScrollState::new(),
            complete: false,
            app_event_tx,
            is_searchable: params.is_searchable,
            search_query: String::new(),
            search_placeholder: if params.is_searchable {
                params.search_placeholder
            } else {
                None
            },
            filtered_indices: Vec::new(),
            last_selected_actual_idx: None,
            header: params.header,
        };
        s.apply_filter();
        s
    }

    fn visible_len(&self) -> usize {
        self.filtered_indices.len()
    }

    fn max_visible_rows(len: usize) -> usize {
        MAX_POPUP_ROWS.min(len.max(1))
    }

    fn apply_filter(&mut self) {
        let previously_selected = self
            .state
            .selected_idx
            .and_then(|visible_idx| self.filtered_indices.get(visible_idx).copied())
            .or_else(|| {
                (!self.is_searchable)
                    .then(|| self.items.iter().position(|item| item.is_current))
                    .flatten()
            });

        if self.is_searchable && !self.search_query.is_empty() {
            let query_lower = self.search_query.to_lowercase();
            self.filtered_indices = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let matches = if let Some(search_value) = &item.search_value {
                        search_value.to_lowercase().contains(&query_lower)
                    } else {
                        let mut matches = item.name.to_lowercase().contains(&query_lower);
                        if !matches && let Some(desc) = &item.description {
                            matches = desc.to_lowercase().contains(&query_lower);
                        }
                        matches
                    };
                    matches.then_some(idx)
                })
                .collect();
        } else {
            self.filtered_indices = (0..self.items.len()).collect();
        }

        let len = self.filtered_indices.len();
        self.state.selected_idx = self
            .state
            .selected_idx
            .and_then(|visible_idx| {
                self.filtered_indices
                    .get(visible_idx)
                    .and_then(|idx| self.filtered_indices.iter().position(|cur| cur == idx))
            })
            .or_else(|| {
                previously_selected.and_then(|actual_idx| {
                    self.filtered_indices
                        .iter()
                        .position(|idx| *idx == actual_idx)
                })
            })
            .or_else(|| (len > 0).then_some(0));

        let visible = Self::max_visible_rows(len);
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, visible);
    }

    fn build_rows(&self) -> Vec<GenericDisplayRow> {
        self.filtered_indices
            .iter()
            .enumerate()
            .filter_map(|(visible_idx, actual_idx)| {
                self.items.get(*actual_idx).map(|item| {
                    let is_selected = self.state.selected_idx == Some(visible_idx);
                    let prefix = if is_selected { '›' } else { ' ' };
                    let name = item.name.as_str();
                    let name_with_marker = if item.is_current {
                        format!("{name} (current)")
                    } else {
                        item.name.clone()
                    };
                    let n = visible_idx + 1;
                    let display_name = format!("{prefix} {n}. {name_with_marker}");
                    GenericDisplayRow {
                        name: display_name,
                        match_indices: None,
                        is_current: item.is_current,
                        description: item.description.clone(),
                    }
                })
            })
            .collect()
    }

    fn move_up(&mut self) {
        let len = self.visible_len();
        self.state.move_up_wrap(len);
        let visible = Self::max_visible_rows(len);
        self.state.ensure_visible(len, visible);
    }

    fn move_down(&mut self) {
        let len = self.visible_len();
        self.state.move_down_wrap(len);
        let visible = Self::max_visible_rows(len);
        self.state.ensure_visible(len, visible);
    }

    fn accept(&mut self) {
        if let Some(idx) = self.state.selected_idx
            && let Some(actual_idx) = self.filtered_indices.get(idx)
            && let Some(item) = self.items.get(*actual_idx)
        {
            self.last_selected_actual_idx = Some(*actual_idx);
            for act in &item.actions {
                act(&self.app_event_tx);
            }
            if item.dismiss_on_select {
                self.complete = true;
            }
        } else {
            self.complete = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn set_search_query(&mut self, query: String) {
        self.search_query = query;
        self.apply_filter();
    }

    pub(crate) fn take_last_selected_index(&mut self) -> Option<usize> {
        self.last_selected_actual_idx.take()
    }

    fn header_lines(&self, width: u16) -> Vec<Line<'static>> {
        if self.header.is_empty() || width == 0 {
            return Vec::new();
        }
        let available = width.max(1) as usize;
        let mut lines = Vec::new();
        for entry in &self.header {
            match entry {
                HeaderLine::Spacer => lines.push(Line::from(String::new())),
                HeaderLine::Text { text, italic } => {
                    if text.is_empty() {
                        lines.push(Line::from(String::new()));
                        continue;
                    }
                    for part in wrap(text, available) {
                        let span = if *italic {
                            Span::from(part.into_owned()).italic()
                        } else {
                            Span::from(part.into_owned())
                        };
                        lines.push(Line::from(vec![span]));
                    }
                }
                HeaderLine::Command { command } => {
                    if command.is_empty() {
                        lines.push(Line::from(String::new()));
                        continue;
                    }
                    let command_width = available.saturating_sub(2).max(1);
                    for (idx, part) in wrap(command, command_width).into_iter().enumerate() {
                        let mut spans = Vec::new();
                        let prefix = if idx == 0 { "$ " } else { "  " };
                        spans.push(Span::from(prefix).dim());
                        spans.push(Span::from(part.into_owned()));
                        lines.push(Line::from(spans));
                    }
                }
            }
        }
        lines
    }

    fn header_height(&self, width: u16) -> u16 {
        self.header_lines(width).len() as u16
    }
}

impl BottomPaneView for ListSelectionView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } if self.is_searchable => {
                self.search_query.pop();
                self.apply_filter();
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.on_ctrl_c();
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            } if self.is_searchable
                && !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.search_query.push(c);
                self.apply_filter();
            }
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

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, width: u16) -> u16 {
        if width < 4 {
            return 3;
        }
        let inner_width = width.saturating_sub(4).max(1);
        let mut height: u16 = 2; // border rows
        height = height.saturating_add(self.header_height(inner_width));
        height = height.saturating_add(1); // title
        if self.is_searchable {
            height = height.saturating_add(1);
        }
        if self.subtitle.is_some() {
            height = height.saturating_add(1);
        }

        let rows = self.build_rows();
        let rows_height = measure_rows_height(&rows, &self.state, MAX_POPUP_ROWS, inner_width);
        if !rows.is_empty() {
            height = height.saturating_add(1); // spacer before rows
        }
        height = height.saturating_add(rows_height);

        if self.footer_hint.is_some() {
            height = height.saturating_add(1);
        }
        height
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        let Some(inner) = draw_history_border(buf, area) else {
            return;
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut cursor_y = inner.y;
        let inner_bottom = inner.y.saturating_add(inner.height);

        for line in self.header_lines(inner.width) {
            if cursor_y >= inner_bottom {
                return;
            }
            Paragraph::new(line).render(
                Rect {
                    x: inner.x,
                    y: cursor_y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
            cursor_y = cursor_y.saturating_add(1);
        }

        if cursor_y >= inner_bottom {
            return;
        }

        Paragraph::new(Line::from(self.title.clone().bold())).render(
            Rect {
                x: inner.x,
                y: cursor_y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        cursor_y = cursor_y.saturating_add(1);

        if self.is_searchable {
            if cursor_y >= inner_bottom {
                return;
            }
            let query_span: Span<'static> = if self.search_query.is_empty() {
                self.search_placeholder
                    .as_ref()
                    .map(|placeholder| placeholder.clone().dim())
                    .unwrap_or_else(|| "".into())
            } else {
                self.search_query.clone().into()
            };
            Paragraph::new(Line::from(vec![query_span])).render(
                Rect {
                    x: inner.x,
                    y: cursor_y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
            cursor_y = cursor_y.saturating_add(1);
        }

        if let Some(sub) = &self.subtitle {
            if cursor_y >= inner_bottom {
                return;
            }
            Paragraph::new(Line::from(sub.clone().dim())).render(
                Rect {
                    x: inner.x,
                    y: cursor_y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
            cursor_y = cursor_y.saturating_add(1);
        }

        let rows = self.build_rows();
        let footer_reserved = self.footer_hint.is_some() as usize;
        let available_rows = inner_bottom.saturating_sub(cursor_y) as usize;

        let mut row_space = available_rows.saturating_sub(footer_reserved);
        if !rows.is_empty() && row_space > 0 {
            if cursor_y >= inner_bottom {
                return;
            }
            Paragraph::new(Line::from(String::new())).render(
                Rect {
                    x: inner.x,
                    y: cursor_y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
            cursor_y = cursor_y.saturating_add(1);
            row_space = row_space.saturating_sub(1);
        }

        if row_space > 0 {
            let rows_area = Rect {
                x: inner.x,
                y: cursor_y,
                width: inner.width,
                height: row_space as u16,
            };
            render_rows(
                rows_area,
                buf,
                &rows,
                &self.state,
                MAX_POPUP_ROWS,
                "no matches",
                false,
            );
        }

        if let Some(hint) = &self.footer_hint {
            if inner.height > 0 {
                Paragraph::new(hint.clone().dim()).render(
                    Rect {
                        x: inner.x,
                        y: inner.y + inner.height - 1,
                        width: inner.width,
                        height: 1,
                    },
                    buf,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BottomPaneView;
    use super::*;
    use crate::app_event::AppEvent;
    use crate::bottom_pane::popup_consts::STANDARD_POPUP_HINT_LINE;
    use insta::assert_snapshot;
    use ratatui::layout::Rect;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_selection_view(subtitle: Option<&str>) -> ListSelectionView {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let items = vec![
            SelectionItem {
                name: "Read Only".to_string(),
                description: Some("Codex can read files".to_string()),
                is_current: true,
                actions: vec![],
                dismiss_on_select: true,
                search_value: None,
            },
            SelectionItem {
                name: "Full Access".to_string(),
                description: Some("Codex can edit files".to_string()),
                is_current: false,
                actions: vec![],
                dismiss_on_select: true,
                search_value: None,
            },
        ];
        ListSelectionView::new(
            SelectionViewParams {
                title: "Select Approval Mode".to_string(),
                subtitle: subtitle.map(str::to_string),
                footer_hint: Some(STANDARD_POPUP_HINT_LINE.to_string()),
                items,
                ..Default::default()
            },
            tx,
        )
    }

    fn render_lines(view: &ListSelectionView) -> String {
        let width = 48;
        let height = BottomPaneView::desired_height(view, width);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let lines: Vec<String> = (0..area.height)
            .map(|row| {
                let mut line = String::new();
                for col in 0..area.width {
                    let symbol = buf[(area.x + col, area.y + row)].symbol();
                    if symbol.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(symbol);
                    }
                }
                line
            })
            .collect();
        lines.join("\n")
    }

    #[test]
    fn renders_blank_line_between_title_and_items_without_subtitle() {
        let view = make_selection_view(None);
        assert_snapshot!(
            "list_selection_spacing_without_subtitle",
            render_lines(&view)
        );
    }

    #[test]
    fn renders_blank_line_between_subtitle_and_items() {
        let view = make_selection_view(Some("Switch between Codex approval presets"));
        assert_snapshot!("list_selection_spacing_with_subtitle", render_lines(&view));
    }

    #[test]
    fn renders_search_query_line_when_enabled() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let items = vec![SelectionItem {
            name: "Read Only".to_string(),
            description: Some("Codex can read files".to_string()),
            is_current: false,
            actions: vec![],
            dismiss_on_select: true,
            search_value: None,
        }];
        let mut view = ListSelectionView::new(
            SelectionViewParams {
                title: "Select Approval Mode".to_string(),
                footer_hint: Some(STANDARD_POPUP_HINT_LINE.to_string()),
                items,
                is_searchable: true,
                search_placeholder: Some("Type to search branches".to_string()),
                ..Default::default()
            },
            tx,
        );
        view.set_search_query("filters".to_string());

        let lines = render_lines(&view);
        assert!(
            lines.lines().any(|line| line.contains("filters")),
            "expected search query to render, got {lines}"
        );
    }
}
