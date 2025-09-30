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
use crate::render::line_utils::push_owned_lines;
use crate::ui_consts::LIVE_PREFIX_COLS;

/// A generic representation of a display row for selection popups.
pub(crate) struct GenericDisplayRow {
    pub name: String,
    pub match_indices: Option<Vec<usize>>, // indices to bold (char positions)
    #[allow(dead_code)]
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

fn wrap_options(desc_col: usize, width: u16) -> RtOptions<'static> {
    RtOptions::new(width as usize)
        .initial_indent(Line::from(String::new()))
        .subsequent_indent(Line::from(" ".repeat(desc_col)))
}

fn wrap_row(row: &GenericDisplayRow, desc_col: usize, width: u16) -> Vec<Line<'static>> {
    let full_line = build_full_line(row, desc_col);
    let wrapped = word_wrap_line(&full_line, wrap_options(desc_col, width));
    let mut owned = Vec::with_capacity(wrapped.len());
    push_owned_lines(&wrapped, &mut owned);
    owned
}

fn wrapped_line_count(row: &GenericDisplayRow, desc_col: usize, width: u16) -> usize {
    let full_line = build_full_line(row, desc_col);
    word_wrap_line(&full_line, wrap_options(desc_col, width)).len()
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
) {
    if include_border {
        use ratatui::widgets::Block;
        use ratatui::widgets::BorderType;
        use ratatui::widgets::Borders;

        // Always draw a dim left border to match other popups.
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::QuadrantOutside)
            .border_style(Style::default().add_modifier(Modifier::DIM));
        block.render(area, buf);
    }

    // Content renders to the right of the border with the same live prefix
    // padding used by the composer so the popup aligns with the input text.
    let prefix_cols = LIVE_PREFIX_COLS;
    let content_area = Rect {
        x: area.x.saturating_add(prefix_cols),
        y: area.y,
        width: area.width.saturating_sub(prefix_cols),
        height: area.height,
    };

    // Clear the padding column(s) so stale characters never peek between the
    // border and the popup contents.
    let padding_cols = prefix_cols.saturating_sub(1);
    if padding_cols > 0 {
        let pad_start = area.x.saturating_add(1);
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

    if rows_all.is_empty() {
        if content_area.height > 0 && content_area.width > 0 {
            Paragraph::new(Line::from(empty_message.dim().italic())).render(
                Rect {
                    x: content_area.x,
                    y: content_area.y,
                    width: content_area.width,
                    height: 1,
                },
                buf,
            );
        }
        return;
    }

    if content_area.width == 0 || content_area.height == 0 {
        return;
    }

    let total_items = rows_all.len();
    let height_limit = content_area.height as usize;
    let max_visible_items = max_results.min(total_items).min(height_limit.max(1));
    if max_visible_items == 0 {
        return;
    }

    let mut start_idx = state.scroll_top.min(total_items.saturating_sub(1));
    if let Some(sel) = state.selected_idx {
        if start_idx > sel {
            start_idx = sel;
        }
    }

    let mut attempts = 0usize;
    let mut chosen_start = start_idx;
    let mut chosen_visible = max_visible_items;
    let mut chosen_desc_col =
        compute_desc_col(rows_all, start_idx, chosen_visible, content_area.width);

    loop {
        attempts = attempts.saturating_add(1);
        if attempts > total_items {
            break;
        }

        let remaining = total_items - start_idx;
        if remaining == 0 {
            break;
        }

        let window_len = max_visible_items.min(remaining);
        if window_len == 0 {
            break;
        }

        let mut desc_col = compute_desc_col(rows_all, start_idx, window_len, content_area.width);
        let mut used_height = 0usize;
        let mut actual_count = 0usize;
        for row in rows_all.iter().skip(start_idx).take(window_len) {
            let line_count = wrapped_line_count(row, desc_col, content_area.width);
            if line_count == 0 {
                continue;
            }
            if used_height + line_count > height_limit {
                break;
            }
            used_height += line_count;
            actual_count += 1;
        }

        if actual_count == 0 {
            actual_count = 1.min(window_len);
        }

        desc_col = compute_desc_col(rows_all, start_idx, actual_count, content_area.width);
        let mut refined_height = 0usize;
        let mut refined_count = 0usize;
        for row in rows_all.iter().skip(start_idx).take(actual_count) {
            let line_count = wrapped_line_count(row, desc_col, content_area.width);
            if line_count == 0 {
                continue;
            }
            if refined_height + line_count > height_limit {
                break;
            }
            refined_height += line_count;
            refined_count += 1;
        }

        if refined_count == 0 {
            refined_count = 1.min(window_len);
        }

        chosen_start = start_idx;
        chosen_visible = refined_count;
        chosen_desc_col = desc_col;

        let selection_visible = state.selected_idx.map_or(true, |sel| {
            sel >= start_idx && sel < start_idx + refined_count
        });

        if selection_visible {
            break;
        }

        if let Some(sel) = state.selected_idx {
            if sel >= start_idx + refined_count {
                if start_idx + 1 >= total_items {
                    break;
                }
                start_idx += 1;
                continue;
            }
            if sel < start_idx {
                if start_idx == 0 {
                    break;
                }
                start_idx -= 1;
                continue;
            }
        }

        break;
    }

    let content_bottom = content_area.y.saturating_add(content_area.height);
    let mut cur_y = content_area.y;
    for (i, row) in rows_all
        .iter()
        .enumerate()
        .skip(chosen_start)
        .take(chosen_visible)
    {
        for mut line in wrap_row(row, chosen_desc_col, content_area.width) {
            if cur_y >= content_bottom {
                return;
            }
            if Some(i) == state.selected_idx {
                line.style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
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
) -> u16 {
    if rows_all.is_empty() {
        return 1; // placeholder "no matches" line
    }

    let content_width = width.saturating_sub(1).max(1);

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

    let mut total: u16 = 0;
    for row in rows_all.iter().skip(start_idx).take(visible_items) {
        let lines = wrapped_line_count(row, desc_col, content_width) as u16;
        total = total.saturating_add(lines.max(1));
    }
    total.max(1)
}
