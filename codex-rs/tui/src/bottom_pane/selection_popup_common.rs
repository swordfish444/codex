use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
// Note: Table-based layout previously used Constraint; the manual renderer
// below no longer requires it.
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

use crate::wrapping::RtOptions;
use crate::wrapping::word_wrap_line;

use super::scroll_state::ScrollState;

/// A generic representation of a display row for selection popups.
pub(crate) struct GenericDisplayRow {
    pub name: String,
    pub match_indices: Option<Vec<usize>>, // indices to bold (char positions)
    pub is_current: bool,
    pub description: Option<String>, // optional grey text after the name
}

impl GenericDisplayRow {}

/// Compute a shared description-column start based on the widest visible name
/// plus two spaces of padding. Ensures at least one column is left for the
/// description.
fn compute_desc_col(
    rows_all: &[GenericDisplayRow],
    start_idx: usize,
    visible_items: usize,
    content_width: u16,
) -> usize {
    let visible_range = start_idx..(start_idx + visible_items);
    let max_name_width = rows_all
        .iter()
        .enumerate()
        .filter(|(i, _)| visible_range.contains(i))
        .map(|(_, r)| Line::from(r.name.clone()).width())
        .max()
        .unwrap_or(0);
    let mut desc_col = max_name_width.saturating_add(2);
    if (desc_col as u16) >= content_width {
        desc_col = content_width.saturating_sub(1) as usize;
    }
    desc_col
}

/// Build the full display line for a row with the description padded to start
/// at `desc_col`. Applies fuzzy-match bolding when indices are present and
/// dims the description.
fn build_full_line(row: &GenericDisplayRow, desc_col: usize) -> Line<'static> {
    // Enforce single-line name: allow at most desc_col - 2 cells for name,
    // reserving two spaces before the description column.
    let name_limit = desc_col.saturating_sub(2);

    let mut name_spans: Vec<Span> = Vec::with_capacity(row.name.len());
    let mut used_width = 0usize;
    let mut truncated = false;

    if let Some(idxs) = row.match_indices.as_ref() {
        let mut idx_iter = idxs.iter().peekable();
        for (char_idx, ch) in row.name.chars().enumerate() {
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_w > name_limit {
                truncated = true;
                break;
            }
            used_width += ch_w;

            if idx_iter.peek().is_some_and(|next| **next == char_idx) {
                idx_iter.next();
                name_spans.push(ch.to_string().bold());
            } else {
                name_spans.push(ch.to_string().into());
            }
        }
    } else {
        for ch in row.name.chars() {
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_w > name_limit {
                truncated = true;
                break;
            }
            used_width += ch_w;
            name_spans.push(ch.to_string().into());
        }
    }

    if truncated {
        // If there is at least one cell available, add an ellipsis.
        // When name_limit is 0, we still show an ellipsis to indicate truncation.
        name_spans.push("…".into());
    }

    let this_name_width = Line::from(name_spans.clone()).width();
    let mut full_spans: Vec<Span> = name_spans;
    if let Some(desc) = row.description.as_ref() {
        let gap = desc_col.saturating_sub(this_name_width);
        if gap > 0 {
            full_spans.push(" ".repeat(gap).into());
        }
        full_spans.push(desc.clone().dim());
    }
    Line::from(full_spans)
}

/// Render a list of rows using the provided ScrollState, with shared styling
/// and behavior for selection popups.
pub(crate) fn render_rows(
    area: Rect,
    buf: &mut Buffer,
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    empty_message: &str,
    include_border: bool,
    prefix_cols: u16,
) {
    if include_border {
        use ratatui::widgets::Block;
        use ratatui::widgets::BorderType;
        use ratatui::widgets::Borders;

        let block = Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::QuadrantOutside)
            .border_style(Style::default().add_modifier(Modifier::DIM));
        block.render(area, buf);
    }

    let content_area = Rect {
        x: area.x.saturating_add(prefix_cols),
        y: area.y,
        width: area.width.saturating_sub(prefix_cols),
        height: area.height,
    };

    let padding_cols = prefix_cols.saturating_sub(if include_border { 1 } else { 0 });
    if padding_cols > 0 {
        let pad_start = if include_border {
            area.x.saturating_add(1)
        } else {
            area.x
        };
        let pad_end = pad_start
            .saturating_add(padding_cols)
            .min(area.x.saturating_add(area.width));
        let pad_bottom = area.y.saturating_add(area.height);
        for x in pad_start..pad_end {
            for y in area.y..pad_bottom {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ");
                }
            }
        }
    }

    if content_area.width == 0 || content_area.height == 0 {
        return;
    }

    if rows_all.is_empty() {
        let para = Paragraph::new(Line::from(empty_message.dim().italic()));
        para.render(
            Rect {
                x: content_area.x,
                y: content_area.y,
                width: content_area.width,
                height: 1,
            },
            buf,
        );
        return;
    }

    let max_rows_from_area = content_area.height as usize;
    let max_items = max_results.min(rows_all.len());

    let sel = state
        .selected_idx
        .unwrap_or(0)
        .min(rows_all.len().saturating_sub(1));

    let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
    if start_idx > sel {
        start_idx = sel;
    }

    let (visible_items, desc_col) = loop {
        let candidate_count = max_items
            .min(rows_all.len().saturating_sub(start_idx))
            .max(1);

        let desc_col_candidate =
            compute_desc_col(rows_all, start_idx, candidate_count, content_area.width);

        let mut used_lines = 0usize;
        let mut temp_visible = 0usize;
        for idx in start_idx..(start_idx + candidate_count) {
            let full_line = build_full_line(&rows_all[idx], desc_col_candidate);
            let options = RtOptions::new(content_area.width as usize)
                .initial_indent(Line::from(""))
                .subsequent_indent(Line::from(" ".repeat(desc_col_candidate)));
            let line_count = word_wrap_line(&full_line, options).len();

            if temp_visible > 0 && used_lines + line_count > max_rows_from_area {
                break;
            }

            if used_lines + line_count > max_rows_from_area && temp_visible == 0 {
                temp_visible = 1;
                break;
            }

            used_lines = used_lines.saturating_add(line_count);
            temp_visible += 1;

            if used_lines >= max_rows_from_area {
                break;
            }
        }

        if temp_visible == 0 {
            temp_visible = 1;
        }

        let end_idx = start_idx + temp_visible - 1;
        if sel <= end_idx || start_idx == sel {
            let desc = compute_desc_col(rows_all, start_idx, temp_visible, content_area.width);
            break (temp_visible, desc);
        }

        if start_idx >= rows_all.len().saturating_sub(1) {
            let desc = compute_desc_col(rows_all, start_idx, temp_visible, content_area.width);
            break (temp_visible, desc);
        }

        start_idx += 1;
    };

    let mut cur_y = content_area.y;
    for (i, row) in rows_all
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(visible_items)
    {
        if cur_y >= content_area.y + content_area.height {
            break;
        }

        let full_line = build_full_line(row, desc_col);
        let options = RtOptions::new(content_area.width as usize)
            .initial_indent(Line::from(""))
            .subsequent_indent(Line::from(" ".repeat(desc_col)));
        let wrapped = word_wrap_line(&full_line, options);

        for mut line in wrapped {
            if cur_y >= content_area.y + content_area.height {
                break;
            }
            if Some(i) == state.selected_idx {
                line.style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
            } else if row.is_current {
                line.style = Style::default().add_modifier(Modifier::ITALIC);
            }
            Paragraph::new(line).render(
                Rect {
                    x: content_area.x,
                    y: cur_y,
                    width: content_area.width,
                    height: 1,
                },
                buf,
            );
            cur_y = cur_y.saturating_add(1);
        }
    }
}
/// Compute the number of terminal rows required to render up to `max_results`
/// items from `rows_all` given the current scroll/selection state and the
/// available `width`. Accounts for description wrapping and alignment so the
/// caller can allocate sufficient vertical space.
pub(crate) fn measure_rows_height(
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    width: u16,
    prefix_cols: u16,
) -> u16 {
    if rows_all.is_empty() {
        return 1;
    }

    let content_width = width.saturating_sub(prefix_cols).max(1);
    let visible_items = max_results.min(rows_all.len());

    let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
    if let Some(sel) = state.selected_idx {
        if sel < start_idx {
            start_idx = sel;
        } else if visible_items > 0 {
            let bottom = start_idx + visible_items - 1;
            if sel > bottom {
                start_idx = sel + 1 - visible_items;
            }
        }
    }

    let desc_col = compute_desc_col(rows_all, start_idx, visible_items, content_width);

    use crate::wrapping::RtOptions;
    use crate::wrapping::word_wrap_line;
    let mut total: u16 = 0;
    for row in rows_all
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(visible_items)
        .map(|(_, r)| r)
    {
        let full_line = build_full_line(row, desc_col);
        let opts = RtOptions::new(content_width as usize)
            .initial_indent(Line::from(""))
            .subsequent_indent(Line::from(" ".repeat(desc_col)));
        total = total.saturating_add(word_wrap_line(&full_line, opts).len() as u16);
    }
    total.max(1)
}
