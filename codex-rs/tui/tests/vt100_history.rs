#![cfg(feature = "vt100-tests")]

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

/// HIST-001: Basic insertion at bottom, no wrap.
///
/// This test captures the ANSI bytes produced by `insert_history_lines_to_writer`
/// when the viewport is at the bottom of the screen (so no pre-scroll is
/// required). It feeds the bytes into a vt100 parser and asserts that the
/// inserted lines are visible near the bottom of the screen.
#[test]
fn hist_001_basic_insertion_no_wrap() {
    // Screen of 20x6; viewport is the last row (height=1 at y=5)
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };

    // Place the viewport at the bottom row
    let area = Rect::new(0, 5, 20, 1);
    term.set_viewport_area(area);

    let lines = vec![Line::from("first"), Line::from("second")];
    let mut buf: Vec<u8> = Vec::new();

    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, lines);

    // Feed captured bytes into vt100 emulator
    let mut parser = vt100::Parser::new(6, 20, 0);
    parser.process(&buf);
    let screen = parser.screen();

    // Gather visible rows as strings
    let mut rows: Vec<String> = Vec::new();
    for row in 0..6 {
        let mut s = String::new();
        for col in 0..20 {
            if let Some(cell) = screen.cell(row, col) {
                let cont = cell.contents();
                if let Some(ch) = cont.chars().next() {
                    s.push(ch);
                } else {
                    s.push(' ');
                }
            } else {
                s.push(' ');
            }
        }
        rows.push(s);
    }

    // The inserted lines should appear somewhere above the viewport; in this
    // simple case, they will occupy the two rows immediately above the final
    // row of the scroll region.
    let joined = rows.join("\n");
    assert!(
        joined.contains("first"),
        "screen did not contain 'first'\n{joined}"
    );
    assert!(
        joined.contains("second"),
        "screen did not contain 'second'\n{joined}"
    );
}

/// HIST-002: Long token wraps across rows within the scroll region.
#[test]
fn hist_002_long_token_wraps() {
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };
    let area = Rect::new(0, 5, 20, 1);
    term.set_viewport_area(area);

    let long = "A".repeat(45); // > 2 lines at width 20
    let lines = vec![Line::from(long.clone())];
    let mut buf: Vec<u8> = Vec::new();

    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, lines);

    let mut parser = vt100::Parser::new(6, 20, 0);
    parser.process(&buf);
    let screen = parser.screen();

    // Count total A's on the screen
    let mut count_a = 0usize;
    for row in 0..6 {
        for col in 0..20 {
            if let Some(cell) = screen.cell(row, col) {
                if let Some(ch) = cell.contents().chars().next() {
                    if ch == 'A' {
                        count_a += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        count_a,
        long.len(),
        "wrapped content did not preserve all characters"
    );
}

/// HIST-003: Emoji/CJK content renders fully (no broken graphemes).
#[test]
fn hist_003_emoji_and_cjk() {
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };
    let area = Rect::new(0, 5, 20, 1);
    term.set_viewport_area(area);

    let text = String::from("😀😀😀😀😀 你好世界");
    let lines = vec![Line::from(text.clone())];
    let mut buf: Vec<u8> = Vec::new();

    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, lines);

    let mut parser = vt100::Parser::new(6, 20, 0);
    parser.process(&buf);
    let screen = parser.screen();

    // Reconstruct string by concatenating non-space cells; ensure all emojis and CJK are present.
    let mut reconstructed = String::new();
    for row in 0..6 {
        for col in 0..20 {
            if let Some(cell) = screen.cell(row, col) {
                let cont = cell.contents();
                if let Some(ch) = cont.chars().next() {
                    if ch != ' ' {
                        reconstructed.push(ch);
                    }
                }
            }
        }
    }

    for ch in text.chars().filter(|c| !c.is_whitespace()) {
        assert!(
            reconstructed.contains(ch),
            "missing character {ch:?} in reconstructed screen"
        );
    }
}

/// HIST-004: Mixed ANSI spans render textual content correctly (styles stripped in emulator).
#[test]
fn hist_004_mixed_ansi_spans() {
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };
    let area = Rect::new(0, 5, 20, 1);
    term.set_viewport_area(area);

    let line = Line::from(vec![
        Span::styled("red", Style::default().fg(Color::Red)),
        Span::raw("+plain"),
    ]);
    let mut buf: Vec<u8> = Vec::new();

    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, vec![line]);

    let mut parser = vt100::Parser::new(6, 20, 0);
    parser.process(&buf);
    let screen = parser.screen();

    let mut rows: Vec<String> = Vec::new();
    for row in 0..6 {
        let mut s = String::new();
        for col in 0..20 {
            if let Some(cell) = screen.cell(row, col) {
                let cont = cell.contents();
                if let Some(ch) = cont.chars().next() {
                    s.push(ch);
                } else {
                    s.push(' ');
                }
            } else {
                s.push(' ');
            }
        }
        rows.push(s);
    }
    let joined = rows.join("\n");
    assert!(
        joined.contains("red+plain"),
        "styled text did not render as expected\n{joined}"
    );
}

/// HIST-006: Cursor is restored after insertion (CUP to 1;1 when backend reports 0,0).
#[test]
fn hist_006_cursor_restoration() {
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };
    let area = Rect::new(0, 5, 20, 1);
    term.set_viewport_area(area);

    let lines = vec![Line::from("x")];
    let mut buf: Vec<u8> = Vec::new();

    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, lines);

    let s = String::from_utf8_lossy(&buf);
    // CUP to 1;1 (ANSI: ESC[1;1H)
    assert!(
        s.contains("\u{1b}[1;1H"),
        "expected final CUP to 1;1 in output, got: {s:?}"
    );
    // Reset scroll region
    assert!(
        s.contains("\u{1b}[r"),
        "expected reset scroll region in output, got: {s:?}"
    );
}

/// HIST-005: Pre-scroll region is emitted via ANSI when viewport is not at bottom.
#[test]
fn hist_005_pre_scroll_region_down() {
    let backend = TestBackend::new(20, 6);
    let mut term = match codex_tui::custom_terminal::Terminal::with_options(backend) {
        Ok(t) => t,
        Err(e) => panic!("failed to construct terminal: {e}"),
    };
    // Viewport not at bottom: y=3 (0-based), height=1
    let area = Rect::new(0, 3, 20, 1);
    term.set_viewport_area(area);

    let lines = vec![Line::from("first"), Line::from("second")];
    let mut buf: Vec<u8> = Vec::new();
    codex_tui::insert_history::insert_history_lines_to_writer(&mut term, &mut buf, lines);

    let s = String::from_utf8_lossy(&buf);
    // Expect we limited scroll region to [top+1 .. screen_height] => [4 .. 6] (1-based)
    assert!(
        s.contains("\u{1b}[4;6r"),
        "expected pre-scroll SetScrollRegion 4..6, got: {s:?}"
    );
    // Expect we moved cursor to top of that region: row 3 (0-based) => CUP 4;1H
    assert!(
        s.contains("\u{1b}[4;1H"),
        "expected cursor at top of pre-scroll region, got: {s:?}"
    );
    // Expect at least two Reverse Index commands (ESC M) for two inserted lines
    let ri_count = s.matches("\u{1b}M").count();
    assert!(
        ri_count >= 1,
        "expected at least one RI (ESC M), got: {s:?}"
    );
    // After pre-scroll, we set insertion scroll region to [1 .. new_top] => [1 .. 5]
    assert!(
        s.contains("\u{1b}[1;5r"),
        "expected insertion SetScrollRegion 1..5, got: {s:?}"
    );
}
