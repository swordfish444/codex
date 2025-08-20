use std::io::Write;

use ratatui::crossterm::execute;
use ratatui::crossterm::style::Print;
use ratatui::crossterm::terminal::Clear;
use ratatui::crossterm::terminal::ClearType;
use unicode_width::UnicodeWidthStr;

static EMOJI_OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Returns true if common emoji we use advance the cursor by the same number of
/// columns as reported by `unicode-width` on this terminal. The value is cached.
pub fn emojis_render_as_expected() -> bool {
    *EMOJI_OK.get_or_init(detect)
}

/// Run a small runtime probe by printing a few glyphs at (0,0) and reading the
/// cursor position. If any measured width differs from `unicode-width`, we
/// conclude emoji rendering is unreliable and the UI should fall back to ASCII.
pub fn detect() -> bool {
    // Only probe a small curated set that we actually use in the UI.
    const TESTS: &[&str] = &["📂", "📖", "🔎", "🧪", "⚡", "⚙︎", "✏︎", "✓", "✗", "🖐"];

    let mut out = std::io::stdout();
    // Best effort: on error, default to false (use ASCII) to avoid broken layout.
    let _ = execute!(out, Clear(ClearType::All));
    for s in TESTS {
        let expected = s.width();
        // Move to origin, print, flush, read cursor position.
        if execute!(out, ratatui::crossterm::cursor::MoveTo(0, 0), Print(*s)).is_err() {
            return false;
        }
        if out.flush().is_err() {
            return false;
        }
        let Ok((x, _y)) = ratatui::crossterm::cursor::position() else {
            return false;
        };
        if x as usize != expected {
            // Clear the line before returning.
            let _ = execute!(
                out,
                ratatui::crossterm::cursor::MoveTo(0, 0),
                Clear(ClearType::CurrentLine)
            );
            return false;
        }
    }
    let _ = execute!(
        out,
        ratatui::crossterm::cursor::MoveTo(0, 0),
        Clear(ClearType::All)
    );
    true
}
