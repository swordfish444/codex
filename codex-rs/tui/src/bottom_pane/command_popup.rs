use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;

use crate::slash_command::SlashCommand;
use crate::slash_command::built_in_slash_commands;
use std::cell::Cell as StdCell;

const MAX_POPUP_ROWS: usize = 5;
/// Ideally this is enough to show the longest command name.
const FIRST_COLUMN_WIDTH: u16 = 20;

use ratatui::style::Modifier;

pub(crate) struct CommandPopup {
    command_filter: String,
    all_commands: Vec<(&'static str, SlashCommand)>,
    selected_idx: Option<usize>,
    /// Index into the filtered command list that indicates the first visible
    /// row in the popup. Ensures the selection remains visible when the list
    /// exceeds MAX_POPUP_ROWS.
    scroll_top: usize,
    /// Number of command rows that fit into the popup given the current
    /// terminal size. Updated on each render.
    visible_rows: StdCell<usize>,
}

impl CommandPopup {
    pub(crate) fn new() -> Self {
        Self {
            command_filter: String::new(),
            all_commands: built_in_slash_commands(),
            selected_idx: None,
            scroll_top: 0,
            visible_rows: StdCell::new(MAX_POPUP_ROWS),
        }
    }

    /// Update the filter string based on the current composer text. The text
    /// passed in is expected to start with a leading '/'. Everything after the
    /// *first* '/" on the *first* line becomes the active filter that is used
    /// to narrow down the list of available commands.
    pub(crate) fn on_composer_text_change(&mut self, text: String) {
        let first_line = text.lines().next().unwrap_or("");

        // Compute new filter token.
        let new_filter = if let Some(stripped) = first_line.strip_prefix('/') {
            let token = stripped.trim_start();
            token.split_whitespace().next().unwrap_or("")
        } else {
            ""
        };

        let prev_filter = self.command_filter.clone();
        self.command_filter = new_filter.to_string();

        let matches_len = self.filtered_commands().len();
        let window = self.visible_rows.get().max(1);

        if self.command_filter == prev_filter {
            // Keep selection/scroll positions stable, but clamp to bounds.
            if matches_len == 0 {
                self.selected_idx = None;
                self.scroll_top = 0;
            } else if let Some(idx) = self.selected_idx {
                let clamped = idx.min(matches_len - 1);
                self.selected_idx = Some(clamped);
                // Ensure scroll_top is within bounds too.
                let max_scroll = matches_len.saturating_sub(window);
                self.scroll_top = self.scroll_top.min(max_scroll);
                if clamped < self.scroll_top {
                    self.scroll_top = clamped;
                }
            } else {
                self.selected_idx = Some(0);
                self.scroll_top = 0;
            }
        } else {
            // Filter changed – reset to top.
            self.selected_idx = if matches_len == 0 { None } else { Some(0) };
            self.scroll_top = 0;
        }
    }

    /// Determine the preferred height of the popup. This is the number of
    /// rows required to show **at most** `MAX_POPUP_ROWS` commands plus the
    /// table/border overhead (one line at the top and one at the bottom).
    pub(crate) fn calculate_required_height(&self, _area: &Rect) -> u16 {
        let matches = self.filtered_commands();
        let row_count = matches.len().clamp(1, MAX_POPUP_ROWS) as u16;
        // Account for the border added by the Block that wraps the table.
        // 2 = one line at the top, one at the bottom.
        row_count + 2
    }

    /// Return the list of commands that match the current filter. Matching is
    /// performed using a *prefix* comparison on the command name.
    fn filtered_commands(&self) -> Vec<&SlashCommand> {
        self.all_commands
            .iter()
            .filter_map(|(_name, cmd)| {
                if self.command_filter.is_empty()
                    || cmd
                        .command()
                        .starts_with(&self.command_filter.to_ascii_lowercase())
                {
                    Some(cmd)
                } else {
                    None
                }
            })
            .collect::<Vec<&SlashCommand>>()
    }

    /// Move the selection cursor one step up.
    pub(crate) fn move_up(&mut self) {
        let matches_len = self.filtered_commands().len();
        let window = self.visible_rows.get().max(1);
        if matches_len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }

        match self.selected_idx {
            Some(0) | None => {
                // Wrap to last element.
                let last = matches_len - 1;
                self.selected_idx = Some(last);
                let max_scroll = matches_len.saturating_sub(window);
                self.scroll_top = max_scroll;
            }
            Some(idx) => {
                let new_idx = idx - 1;
                self.selected_idx = Some(new_idx);
                if new_idx < self.scroll_top {
                    self.scroll_top = new_idx;
                }
            }
        }
    }

    /// Move the selection cursor one step down.
    pub(crate) fn move_down(&mut self) {
        let matches_len = self.filtered_commands().len();
        if matches_len == 0 {
            self.selected_idx = None;
            return;
        }

        let window = self.visible_rows.get().max(1);
        match self.selected_idx {
            None => {
                self.selected_idx = Some(0);
                self.scroll_top = 0;
            }
            Some(idx) => {
                if idx + 1 < matches_len {
                    let new_idx = idx + 1;
                    self.selected_idx = Some(new_idx);
                    if new_idx >= self.scroll_top + window {
                        self.scroll_top = new_idx + 1 - window;
                    }
                } else {
                    // Wrap to first.
                    self.selected_idx = Some(0);
                    self.scroll_top = 0;
                }
            }
        }
    }

    /// Return currently selected command, if any.
    pub(crate) fn selected_command(&self) -> Option<&SlashCommand> {
        let matches = self.filtered_commands();
        self.selected_idx.and_then(|idx| matches.get(idx).copied())
    }
}

impl WidgetRef for CommandPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let matches = self.filtered_commands();

        let mut rows: Vec<Row> = Vec::new();
        // Determine how many rows we can render in the current area (minus border lines).
        let mut visible_rows = area.height.saturating_sub(2) as usize;
        if visible_rows == 0 {
            visible_rows = 1; // Always show at least one row.
        }
        // Persist for key handlers so we can scroll properly.
        self.visible_rows.set(visible_rows);

        let visible_matches: Vec<&SlashCommand> = matches
            .into_iter()
            .skip(self.scroll_top)
            .take(visible_rows)
            .collect();

        if visible_matches.is_empty() {
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from("No matching commands").add_modifier(Modifier::ITALIC),
            ]));
        } else {
            let default_style = Style::default();
            let command_style = Style::default().fg(Color::LightBlue);
            for (visible_idx, cmd) in visible_matches.iter().enumerate() {
                let absolute_idx = self.scroll_top + visible_idx;
                let (cmd_style, desc_style) = if Some(absolute_idx) == self.selected_idx {
                    (
                        command_style.bg(Color::DarkGray),
                        default_style.bg(Color::DarkGray),
                    )
                } else {
                    (command_style, default_style)
                };

                rows.push(Row::new(vec![
                    Cell::from(format!("/{}", cmd.command())).style(cmd_style),
                    Cell::from(cmd.description().to_string()).style(desc_style),
                ]));
            }
        }

        use ratatui::layout::Constraint;

        let table = Table::new(
            rows,
            [Constraint::Length(FIRST_COLUMN_WIDTH), Constraint::Min(10)],
        )
        .column_spacing(0)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );

        table.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_commands_include_compact_when_no_filter() {
        let mut popup = CommandPopup::new();
        popup.on_composer_text_change("/".to_string());
        let cmds = popup.filtered_commands();
        let names: Vec<&str> = cmds.iter().map(|c| c.command()).collect();
        assert!(names.contains(&"compact"));
    }

    #[test]
    fn filtered_commands_only_compact_for_c_prefix() {
        let mut popup = CommandPopup::new();
        popup.on_composer_text_change("/c".to_string());
        let cmds = popup.filtered_commands();
        // Depending on future commands this might include others starting with c.
        // For now ensure that compact is among the top filtered results.
        let names: Vec<&str> = cmds.iter().map(|c| c.command()).collect();
        assert!(names.contains(&"compact"));
    }
}
