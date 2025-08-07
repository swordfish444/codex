use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use tree_sitter::Parser;
use tree_sitter::Tree;
use tree_sitter_bash::LANGUAGE as BASH;

/// Best‑effort syntax highlight for a shell command line.
///
/// Uses tree-sitter-bash (via codex-core) to parse the command and styles
/// common token kinds. Falls back to plain text on parse failure.
fn try_parse_bash(src: &str) -> Option<Tree> {
    let lang = BASH.into();
    let mut parser = Parser::new();
    #[expect(clippy::expect_used)]
    parser.set_language(&lang).expect("load bash grammar");
    parser.parse(src, None)
}

pub(crate) fn highlight_shell_command_line(src: &str) -> Line<'static> {
    let Some(tree) = try_parse_bash(src) else {
        return Line::from(src.to_string());
    };

    // Collect styled segments as byte ranges with a Style.
    #[derive(Clone, Copy)]
    struct Seg {
        start: usize,
        end: usize,
        style: Style,
    }

    let mut segs: Vec<Seg> = Vec::new();

    let root = tree.root_node();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        // We only annotate a handful of common node kinds.
        let kind = node.kind();
        let style = match kind {
            // First word of a command (command_name) stands out.
            "command_name" => Some(
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            // String literals.
            "string" | "raw_string" => Some(Style::default().fg(Color::Green)),
            // Numbers.
            "number" => Some(Style::default().fg(Color::Cyan)),
            // Words: if they look like flags, colour them; else leave for default.
            "word" => {
                if let Ok(text) = node.utf8_text(src.as_bytes()) {
                    if text.starts_with('-') {
                        Some(Style::default().fg(Color::Yellow))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            // Only color operator tokens when they are actual operator nodes.
            _ if !node.is_named() && matches!(kind, "&&" | "||" | "|" | ";" | ">" | "<") => {
                Some(Style::default().fg(Color::Gray))
            }
            _ => None,
        };

        if let Some(style) = style {
            let (start, end) = (node.start_byte(), node.end_byte());
            if start < end && end <= src.len() {
                segs.push(Seg { start, end, style });
            }
            // If we styled a whole string node, skip its children to avoid
            // coloring operator tokens inside strings.
            if matches!(kind, "string" | "raw_string") {
                continue;
            }
        }

        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    // Note: We do NOT globally scan for operator characters; we rely on
    // tree-sitter nodes above so operators inside strings are not colored.

    if segs.is_empty() {
        return Line::from(src.to_string());
    }

    // Merge segments into a sequence of Spans in order, preserving gaps.
    segs.sort_by_key(|s| s.start);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut pos = 0usize;
    for Seg { start, end, style } in segs {
        if start > pos {
            spans.push(Span::raw(src[pos..start].to_string()));
        }
        let piece = &src[start..end];
        spans.push(Span::styled(piece.to_string(), style));
        pos = end;
    }
    if pos < src.len() {
        spans.push(Span::raw(src[pos..].to_string()));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn highlight_does_not_color_operators_inside_strings() {
        // Example provided by user: regex pipes should remain inside a green string span,
        // and not be treated as shell operators.
        let cmd = r#"rg -n --no-ignore-vcs -S "TODO|FIXME|XXX|HACK|TBD|\bBUG\b|\bunimplemented!\(|\btodo!\(""#;

        let line = highlight_shell_command_line(cmd);

        // Reconstruct text
        let reconstructed: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(reconstructed, cmd);

        // There should be no gray operator tokens, since all pipes are inside quotes.
        let has_gray_ops = line.spans.iter().any(|s| {
            s.style.fg == Some(Color::Gray) && (s.content.contains('|') || s.content.contains("||"))
        });
        assert!(
            !has_gray_ops,
            "found gray operator tokens inside quoted regex"
        );

        // There should be at least one green span that contains a pipe character from the regex.
        let has_green_with_pipe = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Green) && s.content.contains('|'));
        assert!(
            has_green_with_pipe,
            "expected quoted regex to be highlighted as a green string"
        );
    }

    #[test]
    fn highlight_colors_command_and_flags() {
        let cmd = "rg -n --no-ignore-vcs foo";
        let line = highlight_shell_command_line(cmd);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect();
        assert_eq!(text, cmd);

        // Find first token 'rg' and ensure it's blue (command name)
        let has_blue_cmd = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Blue) && s.content.contains("rg"));
        assert!(has_blue_cmd, "expected command name to be blue");

        // Flags should be yellow
        let has_short_flag = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Yellow) && s.content.contains("-n"));
        let has_long_flag = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Yellow) && s.content.contains("--no-ignore-vcs"));
        assert!(
            has_short_flag && has_long_flag,
            "expected flags to be yellow"
        );
    }

    #[test]
    fn highlight_colors_numbers() {
        let cmd = "echo 123 456";
        let line = highlight_shell_command_line(cmd);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect();
        assert_eq!(text, cmd);

        let has_cyan_numbers = line.spans.iter().any(|s| {
            s.style.fg == Some(Color::Cyan)
                && (s.content.contains("123") || s.content.contains("456"))
        });
        assert!(has_cyan_numbers, "expected numbers to be cyan");
    }

    #[test]
    fn highlight_colors_operators_outside_strings() {
        let cmd = "echo a | grep b && true;";
        let line = highlight_shell_command_line(cmd);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect();
        assert_eq!(text, cmd);

        // Operators outside strings should be gray
        let has_gray_ops = line.spans.iter().any(|s| {
            s.style.fg == Some(Color::Gray)
                && (s.content.contains("|") || s.content.contains("&&") || s.content.contains(";"))
        });
        assert!(
            has_gray_ops,
            "expected operators outside strings to be gray"
        );
    }

    #[test]
    fn highlight_handles_redirections() {
        let cmd = "cat file > out && echo ok";
        let line = highlight_shell_command_line(cmd);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect();
        assert_eq!(text, cmd);

        let has_gray_redirect = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Gray) && s.content.contains(">"));
        assert!(has_gray_redirect, "expected '>' to be colored (gray)");
    }

    #[test]
    fn highlight_multiple_quoted_strings() {
        let cmd = "echo \"a|b\" 'c|d'";
        let line = highlight_shell_command_line(cmd);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.clone().into_owned())
            .collect();
        assert_eq!(text, cmd);

        let green_spans_with_pipes = line
            .spans
            .iter()
            .filter(|s| s.style.fg == Some(Color::Green) && s.content.contains("|"))
            .count();
        assert!(
            green_spans_with_pipes >= 2,
            "expected both quoted strings to be green with pipes"
        );

        let has_gray_pipes = line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Gray) && s.content.contains("|"));
        assert!(
            !has_gray_pipes,
            "should not color '|' as operator inside quotes"
        );
    }
}
