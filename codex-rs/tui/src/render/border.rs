use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;

/// Draw the standard Codex rounded border into `buf` and return the interior
/// rectangle available for content. When the area is too small to hold the
/// border (width < 4 or height < 3) this returns `None` and leaves the buffer
/// untouched.
pub(crate) fn draw_history_border(buf: &mut Buffer, area: Rect) -> Option<Rect> {
    if area.width < 4 || area.height < 3 {
        return None;
    }

    let style = Style::default().add_modifier(Modifier::DIM);
    let left = area.x;
    let right = area.x + area.width - 1;
    let top = area.y;
    let bottom = area.y + area.height - 1;

    // Top border
    buf[(left, top)].set_symbol("╭").set_style(style);
    for x in left + 1..right {
        buf[(x, top)].set_symbol("─").set_style(style);
    }
    buf[(right, top)].set_symbol("╮").set_style(style);

    // Bottom border
    buf[(left, bottom)].set_symbol("╰").set_style(style);
    for x in left + 1..right {
        buf[(x, bottom)].set_symbol("─").set_style(style);
    }
    buf[(right, bottom)].set_symbol("╯").set_style(style);

    // Sides + clear interior padding columns
    for y in top + 1..bottom {
        buf[(left, y)].set_symbol("│").set_style(style);
        buf[(right, y)].set_symbol("│").set_style(style);

        // Left padding column
        buf[(left + 1, y)].set_symbol(" ").set_style(style);
        // Right padding column
        buf[(right - 1, y)].set_symbol(" ").set_style(style);

        // Interior content area reset to spaces
        for x in left + 2..right - 1 {
            buf[(x, y)].set_symbol(" ").set_style(Style::default());
        }
    }

    Some(Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width - 4,
        height: area.height - 2,
    })
}
