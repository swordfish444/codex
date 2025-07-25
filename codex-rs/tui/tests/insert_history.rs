use codex_tui::insert_history_lines;
use ratatui::backend::TestBackend;
use ratatui::text::{Line, Span};
use ratatui::{Terminal, TerminalOptions, Viewport};
use ratatui::widgets::Paragraph;

// Helper to initialise a terminal with an inline viewport.
fn test_terminal(width: u16, height: u16, bottom_height: u16) -> Terminal<TestBackend> {
    Terminal::with_options(
        TestBackend::new(width, height),
        TerminalOptions { viewport: Viewport::Inline(bottom_height) },
    )
    .expect("terminal")
}

// Extract the buffer contents as Strings (one per row) trimming trailing spaces.
fn buffer_lines(term: &Terminal<TestBackend>) -> Vec<String> {
    let backend = term.backend();
    let size = term.size().expect("size");
    let mut out = Vec::new();
    for y in 0..size.height {
        let mut row = String::new();
        for x in 0..size.width {
            let cell = backend.buffer().get(x, y);
            row.push_str(cell.symbol());
        }
        out.push(row.trim_end().to_string());
    }
    out
}

#[test]
fn single_line_passthrough() {
    let mut term = test_terminal(20, 10, 3); // 7 lines history space
    insert_history_lines(&mut term, vec![Line::from("hello world")]);
    let lines = buffer_lines(&term);
    assert!(lines.iter().any(|l| l.contains("hello world")), "history line visible");
}

#[test]
fn explicit_newlines_preserved() {
    let mut term = test_terminal(20, 10, 3);
    insert_history_lines(&mut term, vec![Line::from(Span::raw("foo\nbar\n"))]);
    let lines = buffer_lines(&term);
    assert!(lines.contains(&"foo".to_string()));
    assert!(lines.contains(&"bar".to_string()));
    assert!(lines.iter().filter(|l| l.is_empty()).count() >= 1);
}

#[test]
fn whitespace_normalisation() {
    let mut term = test_terminal(30, 10, 3);
    insert_history_lines(
        &mut term,
        vec![Line::from(vec![Span::raw("   a"), Span::raw("\t\tb"), Span::raw("   c")])],
    );
    let joined = buffer_lines(&term).join("\n");
    assert!(joined.contains("a b c"));
}

#[test]
fn soft_wrapping() {
    let mut term = test_terminal(10, 10, 3);
    insert_history_lines(&mut term, vec![Line::from("hello world test")]);
    let lines = buffer_lines(&term);
    assert!(lines.iter().any(|l| l == "hello"));
    assert!(lines.iter().any(|l| l == "world test"));
}

#[test]
fn overlong_word_splitting() {
    let mut term = test_terminal(5, 10, 3);
    insert_history_lines(&mut term, vec![Line::from("abcdefgh")]);
    let lines = buffer_lines(&term);
    assert!(lines.iter().any(|l| l == "abcde"));
    assert!(lines.iter().any(|l| l == "fgh"));
}

#[test]
fn whitespace_collapse_across_spans() {
    let mut term = test_terminal(20, 10, 3);
    insert_history_lines(&mut term, vec![Line::from(vec![Span::raw("foo "), Span::raw("   bar")])]);
    let joined = buffer_lines(&term).join("\n");
    assert!(joined.contains("foo bar"));
    assert!(!joined.contains("foo   bar"));
}

#[test]
fn trailing_newline_preserved() {
    let mut term = test_terminal(20, 10, 3);
    insert_history_lines(&mut term, vec![Line::from(Span::raw("xyz\n"))]);
    let lines = buffer_lines(&term);
    assert!(lines.contains(&"xyz".to_string()));
    assert!(lines.iter().filter(|l| l.is_empty()).count() >= 1);
}

#[test]
fn wide_unicode_wrapping() {
    let mut term = test_terminal(6, 10, 3);
    insert_history_lines(&mut term, vec![Line::from("ＡＢＣＤＥ")]);
    let lines = buffer_lines(&term);
    assert!(lines.iter().any(|l| l.contains("Ａ Ｂ Ｃ")));
    assert!(lines.iter().any(|l| l.contains("Ｄ Ｅ")));
}

#[test]
fn sequential_insertions_order() {
    let mut term = test_terminal(20, 10, 3);
    insert_history_lines(&mut term, vec![Line::from("first")]);
    insert_history_lines(&mut term, vec![Line::from("second")]);
    let lines = buffer_lines(&term);
    let mut first_idx = None;
    let mut second_idx = None;
    for (i, l) in lines.iter().enumerate() {
        if l.contains("first") { first_idx = Some(i); }
        if l.contains("second") { second_idx = Some(i); }
    }
    let (Some(fi), Some(si)) = (first_idx, second_idx) else { panic!("missing lines") };
    assert!(fi < si, "expected 'first' above 'second'");
}

#[test]
fn integration_bottom_viewport_render() {
    let mut term = test_terminal(15, 8, 3);
    insert_history_lines(&mut term, vec![Line::from("history one"), Line::from("history two")]);
    term.draw(|f| f.render_widget(Paragraph::new("bottom"), f.area())).unwrap();
    let lines = buffer_lines(&term);
    assert!(lines.iter().any(|l| l.contains("history one")));
    assert!(lines.iter().any(|l| l.contains("history two")));
    assert!(lines.iter().any(|l| l.contains("bottom")));
}

