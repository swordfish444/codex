use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;

/// Draw the standard Codex rounded border into `buf` and return the interior
/// rectangle where content should render. The border mirrors the appearance of
/// `history_cell::with_border`, including one column of padding on each side.
pub(crate) fn draw_history_border(buf: &mut Buffer, area: Rect) -> Option<Rect> {
    if area.width < 4 || area.height < 3 {
        return None;
    }

    let dim_style = Style::default().add_modifier(Modifier::DIM);

    let left = area.x;
    let right = area.x + area.width - 1;
    let top = area.y;
    let bottom = area.y + area.height - 1;

    if let Some(cell) = buf.cell_mut((left, top)) {
        cell.set_symbol("╭");
        cell.set_style(dim_style);
    }
    for x in left + 1..right {
        if let Some(cell) = buf.cell_mut((x, top)) {
            cell.set_symbol("─");
            cell.set_style(dim_style);
        }
    }
    if let Some(cell) = buf.cell_mut((right, top)) {
        cell.set_symbol("╮");
        cell.set_style(dim_style);
    }

    if let Some(cell) = buf.cell_mut((left, bottom)) {
        cell.set_symbol("╰");
        cell.set_style(dim_style);
    }
    for x in left + 1..right {
        if let Some(cell) = buf.cell_mut((x, bottom)) {
            cell.set_symbol("─");
            cell.set_style(dim_style);
        }
    }
    if let Some(cell) = buf.cell_mut((right, bottom)) {
        cell.set_symbol("╯");
        cell.set_style(dim_style);
    }

    for y in top + 1..bottom {
        if let Some(cell) = buf.cell_mut((left, y)) {
            cell.set_symbol("│");
            cell.set_style(dim_style);
        }
        if let Some(cell) = buf.cell_mut((left + 1, y)) {
            cell.set_symbol(" ");
            cell.set_style(dim_style);
        }
        for x in left + 2..right - 1 {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(" ");
                cell.set_style(Style::default());
            }
        }
        if let Some(cell) = buf.cell_mut((right - 1, y)) {
            cell.set_symbol(" ");
            cell.set_style(dim_style);
        }
        if let Some(cell) = buf.cell_mut((right, y)) {
            cell.set_symbol("│");
            cell.set_style(dim_style);
        }
    }

    Some(Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    })
}
