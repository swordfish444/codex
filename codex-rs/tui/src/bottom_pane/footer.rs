use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::WidgetRef;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FooterProps {
    pub(crate) mode: FooterMode,
    pub(crate) esc_backtrack_hint: bool,
    pub(crate) use_shift_enter_hint: bool,
    pub(crate) is_task_running: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FooterMode {
    CtrlCReminder,
    ShortcutPrompt,
    ShortcutOverlay,
    EscHint,
}

#[derive(Clone, Copy, Debug)]
struct CtrlCReminderState {
    is_task_running: bool,
}

#[derive(Clone, Copy, Debug)]
struct ShortcutsState {
    use_shift_enter_hint: bool,
    esc_backtrack_hint: bool,
    is_task_running: bool,
}

struct ShortcutEntry {
    render: fn(ShortcutsState) -> Option<String>,
}

const SHORTCUT_ENTRIES: &[ShortcutEntry] = &[
    ShortcutEntry {
        render: |_: ShortcutsState| Some("/ for commands".to_string()),
    },
    ShortcutEntry {
        render: |_: ShortcutsState| Some("@ for file paths".to_string()),
    },
    ShortcutEntry {
        render: |state: ShortcutsState| {
            let binding = if state.use_shift_enter_hint {
                "shift + enter"
            } else {
                "ctrl + j"
            };
            Some(format!("{binding} for newline"))
        },
    },
    ShortcutEntry {
        render: |_: ShortcutsState| Some("ctrl + v to paste images".to_string()),
    },
    ShortcutEntry {
        render: |state: ShortcutsState| {
            let action = if state.is_task_running {
                "interrupt"
            } else {
                "exit"
            };
            Some(format!("ctrl + c to {action}"))
        },
    },
    ShortcutEntry {
        render: |_: ShortcutsState| Some("ctrl + t to view transcript".to_string()),
    },
    ShortcutEntry {
        render: |_: ShortcutsState| Some("? to hide shortcuts".to_string()),
    },
    ShortcutEntry {
        render: |state: ShortcutsState| {
            let label = if state.esc_backtrack_hint {
                "esc again to edit previous message"
            } else {
                "esc esc to edit previous message"
            };
            Some(label.to_string())
        },
    },
];

pub(crate) fn footer_height(props: &FooterProps) -> u16 {
    footer_lines(props).len() as u16
}

pub(crate) fn render_footer(area: Rect, buf: &mut Buffer, props: FooterProps) {
    let lines = footer_lines(&props);
    for (idx, line) in lines.into_iter().enumerate() {
        let y = area.y + idx as u16;
        if y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, y, area.width, 1);
        line.render_ref(row, buf);
    }
}

fn footer_lines(props: &FooterProps) -> Vec<Line<'static>> {
    match props.mode {
        FooterMode::CtrlCReminder => {
            vec![ctrl_c_reminder_line(CtrlCReminderState {
                is_task_running: props.is_task_running,
            })]
        }
        FooterMode::ShortcutPrompt => vec![Line::from(vec!["? for shortcuts".dim()])],
        FooterMode::ShortcutOverlay => shortcut_overlay_lines(ShortcutsState {
            use_shift_enter_hint: props.use_shift_enter_hint,
            esc_backtrack_hint: props.esc_backtrack_hint,
            is_task_running: props.is_task_running,
        }),
        FooterMode::EscHint => {
            vec![esc_hint_line(ShortcutsState {
                use_shift_enter_hint: props.use_shift_enter_hint,
                esc_backtrack_hint: props.esc_backtrack_hint,
                is_task_running: props.is_task_running,
            })]
        }
    }
}

fn ctrl_c_reminder_line(state: CtrlCReminderState) -> Line<'static> {
    let action = if state.is_task_running {
        "interrupt"
    } else {
        "quit"
    };
    Line::from(vec![
        Span::from(format!("  ctrl + c again to {action}")).dim(),
    ])
}

fn shortcut_overlay_lines(state: ShortcutsState) -> Vec<Line<'static>> {
    let mut rendered = Vec::new();
    for entry in SHORTCUT_ENTRIES {
        if let Some(text) = (entry.render)(state) {
            rendered.push(text);
        }
    }
    build_columns(rendered)
}

fn esc_hint_line(state: ShortcutsState) -> Line<'static> {
    let text = if state.esc_backtrack_hint {
        "  esc again to edit previous message"
    } else {
        "  esc esc to edit previous message"
    };
    Line::from(vec![Span::from(text).dim()])
}

fn build_columns(entries: Vec<String>) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return Vec::new();
    }

    const COLUMNS: usize = 3;
    const MAX_PADDED_WIDTHS: [usize; COLUMNS - 1] = [24, 28];

    let rows = (entries.len() + COLUMNS - 1) / COLUMNS;
    let mut column_widths = vec![0usize; COLUMNS];

    for (idx, entry) in entries.iter().enumerate() {
        let column = idx % COLUMNS;
        column_widths[column] = column_widths[column].max(entry.len());
    }

    let mut lines = Vec::new();
    for row in 0..rows {
        let mut line = String::from("  ");
        for col in 0..COLUMNS {
            let idx = row * COLUMNS + col;
            if idx >= entries.len() {
                continue;
            }
            let entry = &entries[idx];
            if col < COLUMNS - 1 {
                let max_width = MAX_PADDED_WIDTHS[col];
                let target_width = column_widths[col].min(max_width);
                let pad_width = target_width + 2;
                line.push_str(&format!("{entry:<pad_width$}", pad_width = pad_width));
            } else {
                if col != 0 {
                    line.push_str("  ");
                }
                line.push_str(entry);
            }
        }
        lines.push(Line::from(vec![Span::from(line).dim()]));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn snapshot_footer(name: &str, props: FooterProps) {
        let height = footer_height(&props).max(1);
        let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, f.area().width, height);
                render_footer(area, f.buffer_mut(), props);
            })
            .unwrap();
        assert_snapshot!(name, terminal.backend());
    }

    #[test]
    fn footer_snapshots() {
        snapshot_footer(
            "footer_shortcuts_default",
            FooterProps {
                mode: FooterMode::ShortcutPrompt,
                esc_backtrack_hint: false,
                use_shift_enter_hint: false,
                is_task_running: false,
            },
        );

        snapshot_footer(
            "footer_shortcuts_shift_and_esc",
            FooterProps {
                mode: FooterMode::ShortcutOverlay,
                esc_backtrack_hint: true,
                use_shift_enter_hint: true,
                is_task_running: false,
            },
        );

        snapshot_footer(
            "footer_ctrl_c_quit_idle",
            FooterProps {
                mode: FooterMode::CtrlCReminder,
                esc_backtrack_hint: false,
                use_shift_enter_hint: false,
                is_task_running: false,
            },
        );

        snapshot_footer(
            "footer_ctrl_c_quit_running",
            FooterProps {
                mode: FooterMode::CtrlCReminder,
                esc_backtrack_hint: false,
                use_shift_enter_hint: false,
                is_task_running: true,
            },
        );

        snapshot_footer(
            "footer_esc_hint_idle",
            FooterProps {
                mode: FooterMode::EscHint,
                esc_backtrack_hint: false,
                use_shift_enter_hint: false,
                is_task_running: false,
            },
        );

        snapshot_footer(
            "footer_esc_hint_primed",
            FooterProps {
                mode: FooterMode::EscHint,
                esc_backtrack_hint: true,
                use_shift_enter_hint: false,
                is_task_running: false,
            },
        );
    }
}
