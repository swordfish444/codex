use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;
use crate::slash_command::SlashCommand;
use crate::slash_command::built_in_slash_commands;
use codex_common::fuzzy_match::fuzzy_match;
use std::fs;
use std::path::PathBuf;

/// A discovered prompt in ~/.codex/prompts.
#[derive(Clone, Debug)]
pub(crate) struct PromptEntry {
    pub name: String,
    pub path: PathBuf,
}

/// A selectable item in the popup: either a built-in command or a user prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandItem {
    Builtin(SlashCommand),
    /// Index into `prompts` vector of `CommandPopup`.
    Prompt(usize),
}

pub(crate) struct CommandPopup {
    command_filter: String,
    builtins: Vec<(&'static str, SlashCommand)>,
    prompts: Vec<PromptEntry>,
    state: ScrollState,
}

impl CommandPopup {
    pub(crate) fn new() -> Self {
        let builtins = built_in_slash_commands();
        let prompts = Self::discover_prompts(&builtins);
        Self {
            command_filter: String::new(),
            builtins,
            prompts,
            state: ScrollState::new(),
        }
    }

    /// If `idx` refers to a prompt item, return its absolute file path.
    pub(crate) fn prompt_path(&self, idx: usize) -> Option<&PathBuf> {
        self.prompts.get(idx).map(|p| &p.path)
    }

    pub(crate) fn prompt_name(&self, idx: usize) -> Option<&str> {
        self.prompts.get(idx).map(|p| p.name.as_str())
    }

    /// Scan ~/.codex/prompts for files and return them as prompt entries.
    fn discover_prompts(builtins: &Vec<(&'static str, SlashCommand)>) -> Vec<PromptEntry> {
        // Build a set of builtin names to avoid collisions.
        let mut builtin_names = std::collections::HashSet::new();
        for (name, _) in builtins.iter() {
            builtin_names.insert(*name);
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let prompts_dir = PathBuf::from(format!("{home}/.codex/prompts"));
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&prompts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_name = path.file_stem().and_then(|s| s.to_str());
                let Some(name) = file_name else {
                    continue;
                };
                // Avoid duplicates with built-ins; prefer built-ins.
                if builtin_names.contains(name) {
                    continue;
                }
                out.push(PromptEntry {
                    name: name.to_string(),
                    path,
                });
            }
            // Sort prompts by name for stable ordering.
            out.sort_by(|a, b| a.name.cmp(&b.name));
        }
        out
    }

    /// Update the filter string based on the current composer text. The text
    /// passed in is expected to start with a leading '/'. Everything after the
    /// *first* '/" on the *first* line becomes the active filter that is used
    /// to narrow down the list of available commands.
    pub(crate) fn on_composer_text_change(&mut self, text: String) {
        let first_line = text.lines().next().unwrap_or("");

        if let Some(stripped) = first_line.strip_prefix('/') {
            // Extract the *first* token (sequence of non-whitespace
            // characters) after the slash so that `/clear something` still
            // shows the help for `/clear`.
            let token = stripped.trim_start();
            let cmd_token = token.split_whitespace().next().unwrap_or("");

            // Update the filter keeping the original case (commands are all
            // lower-case for now but this may change in the future).
            self.command_filter = cmd_token.to_string();
        } else {
            // The composer no longer starts with '/'. Reset the filter so the
            // popup shows the *full* command list if it is still displayed
            // for some reason.
            self.command_filter.clear();
        }

        // Reset or clamp selected index based on new filtered list.
        let matches_len = self.filtered_items().len();
        self.state.clamp_selection(matches_len);
        self.state
            .ensure_visible(matches_len, MAX_POPUP_ROWS.min(matches_len));
    }

    /// Determine the preferred height of the popup. This is the number of
    /// rows required to show at most MAX_POPUP_ROWS commands.
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.filtered_items().len().clamp(1, MAX_POPUP_ROWS) as u16
    }

    /// Compute fuzzy-filtered matches paired with optional highlight indices and score.
    /// Sorted by ascending score, then by command name for stability.
    fn filtered(&self) -> Vec<(CommandItem, Option<Vec<usize>>, i32)> {
        let filter = self.command_filter.trim();
        let mut out: Vec<(CommandItem, Option<Vec<usize>>, i32)> = Vec::new();
        if filter.is_empty() {
            // Built-ins first, in presentation order.
            for (_, cmd) in self.builtins.iter() {
                out.push((CommandItem::Builtin(*cmd), None, 0));
            }
            // Then prompts, already sorted by name.
            for idx in 0..self.prompts.len() {
                out.push((CommandItem::Prompt(idx), None, 0));
            }
            return out;
        }

        for (_, cmd) in self.builtins.iter() {
            if let Some((indices, score)) = fuzzy_match(cmd.command(), filter) {
                out.push((CommandItem::Builtin(*cmd), Some(indices), score));
            }
        }
        for (idx, p) in self.prompts.iter().enumerate() {
            if let Some((indices, score)) = fuzzy_match(&p.name, filter) {
                out.push((CommandItem::Prompt(idx), Some(indices), score));
            }
        }
        // When filtering, sort by ascending score and then by name for stability.
        out.sort_by(|a, b| {
            let an = match a.0 {
                CommandItem::Builtin(c) => c.command(),
                CommandItem::Prompt(i) => &self.prompts[i].name,
            };
            let bn = match b.0 {
                CommandItem::Builtin(c) => c.command(),
                CommandItem::Prompt(i) => &self.prompts[i].name,
            };
            a.2.cmp(&b.2).then_with(|| an.cmp(bn))
        });
        out
    }

    fn filtered_items(&self) -> Vec<CommandItem> {
        self.filtered().into_iter().map(|(c, _, _)| c).collect()
    }

    /// Move the selection cursor one step up.
    pub(crate) fn move_up(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    /// Move the selection cursor one step down.
    pub(crate) fn move_down(&mut self) {
        let matches_len = self.filtered_items().len();
        self.state.move_down_wrap(matches_len);
        self.state
            .ensure_visible(matches_len, MAX_POPUP_ROWS.min(matches_len));
    }

    /// Return currently selected command, if any.
    pub(crate) fn selected_item(&self) -> Option<CommandItem> {
        let matches = self.filtered_items();
        self.state
            .selected_idx
            .and_then(|idx| matches.get(idx).copied())
    }
}

impl WidgetRef for CommandPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let matches = self.filtered();
        let rows_all: Vec<GenericDisplayRow> = if matches.is_empty() {
            Vec::new()
        } else {
            matches
                .into_iter()
                .map(|(item, indices, _)| match item {
                    CommandItem::Builtin(cmd) => GenericDisplayRow {
                        name: format!("/{}", cmd.command()),
                        match_indices: indices.map(|v| v.into_iter().map(|i| i + 1).collect()),
                        is_current: false,
                        description: Some(cmd.description().to_string()),
                    },
                    CommandItem::Prompt(i) => GenericDisplayRow {
                        name: format!("/{}", self.prompts[i].name),
                        match_indices: indices.map(|v| v.into_iter().map(|i| i + 1).collect()),
                        is_current: false,
                        description: Some("send saved prompt".to_string()),
                    },
                })
                .collect()
        };
        render_rows(area, buf, &rows_all, &self.state, MAX_POPUP_ROWS, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn filter_includes_init_when_typing_prefix() {
        let mut popup = CommandPopup::new();
        // Simulate the composer line starting with '/in' so the popup filters
        // matching commands by prefix.
        popup.on_composer_text_change("/in".to_string());

        // Access the filtered list via the selected command and ensure that
        // one of the matches is the new "init" command.
        let matches = popup.filtered_items();
        let has_init = matches.iter().any(|item| match item {
            CommandItem::Builtin(cmd) => cmd.command() == "init",
            CommandItem::Prompt(_) => false,
        });
        assert!(
            has_init,
            "expected '/init' to appear among filtered commands"
        );
    }

    #[test]
    fn selecting_init_by_exact_match() {
        let mut popup = CommandPopup::new();
        popup.on_composer_text_change("/init".to_string());

        // When an exact match exists, the selected command should be that
        // command by default.
        let selected = popup.selected_item();
        match selected {
            Some(CommandItem::Builtin(cmd)) => assert_eq!(cmd.command(), "init"),
            Some(CommandItem::Prompt(_)) => panic!("unexpected prompt selected for '/init'"),
            None => panic!("expected a selected command for exact match"),
        }
    }

    #[test]
    fn prompt_discovery_lists_custom_prompts() {
        let tmp = tempdir().expect("create TempDir");
        let home = tmp.path();
        let prompts_dir = home.join(".codex").join("prompts");
        std::fs::create_dir_all(&prompts_dir).expect("mkdir -p ~/.codex/prompts");
        std::fs::write(prompts_dir.join("foo"), b"hello from foo").unwrap();
        std::fs::write(prompts_dir.join("bar"), b"hello from bar").unwrap();

        // Point HOME to the temp dir so discovery uses our fixtures.
        unsafe { std::env::set_var("HOME", home) };

        let popup = CommandPopup::new();
        let items = popup.filtered_items();
        let mut prompt_names: Vec<String> = items
            .into_iter()
            .filter_map(|it| match it {
                CommandItem::Prompt(i) => popup.prompt_name(i).map(|s| s.to_string()),
                _ => None,
            })
            .collect();
        prompt_names.sort();
        assert_eq!(prompt_names, vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn prompt_name_collision_with_builtin_is_ignored() {
        let tmp = tempdir().expect("create TempDir");
        let home = tmp.path();
        let prompts_dir = home.join(".codex").join("prompts");
        std::fs::create_dir_all(&prompts_dir).expect("mkdir -p ~/.codex/prompts");
        // Create a prompt with the same name as a builtin command (e.g. "init").
        std::fs::write(prompts_dir.join("init"), b"should be ignored").unwrap();

        unsafe { std::env::set_var("HOME", home) };

        let popup = CommandPopup::new();
        let items = popup.filtered_items();
        let has_collision_prompt = items.into_iter().any(|it| match it {
            CommandItem::Prompt(i) => popup.prompt_name(i) == Some("init"),
            _ => false,
        });
        assert!(
            !has_collision_prompt,
            "prompt with builtin name should be ignored"
        );
    }
}
