use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use tui_textarea::Input;
use tui_textarea::Key;
use tui_textarea::TextArea;

use super::chat_composer_history::ChatComposerHistory;
use super::command_popup::CommandPopup;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::slash_command::SlashCommand;

/// Minimum number of visible text rows inside the textarea.
const MIN_TEXTAREA_ROWS: usize = 1;
/// Rows consumed by the border.
const BORDER_LINES: u16 = 2;

/// Result returned when the user interacts with the text area.
pub enum InputResult {
    Submitted(String),
    None,
}

pub(crate) struct ChatComposer<'a> {
    textarea: TextArea<'a>,
    command_popup: Option<CommandPopup>,
    app_event_tx: AppEventSender,
    history: ChatComposerHistory,
    /// Maximum number of visible lines in the chat input composer.
    max_rows: usize,
    /// Last computed context-left percentage
    context_left_percent: f64,
}

impl ChatComposer<'_> {
    pub fn new(has_input_focus: bool, app_event_tx: AppEventSender, max_rows: usize) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("send a message");
        textarea.set_cursor_line_style(ratatui::style::Style::default());

        let mut this = Self {
            textarea,
            command_popup: None,
            app_event_tx,
            history: ChatComposerHistory::new(),
            max_rows,
            context_left_percent: 100.0,
        };
        this.update_border(has_input_focus);
        this
    }

    /// Record the history metadata advertised by `SessionConfiguredEvent` so
    /// that the composer can navigate cross-session history.
    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history.set_metadata(log_id, entry_count);
    }

    /// Integrate an asynchronous response to an on-demand history lookup. If
    /// the entry is present and the offset matches the current cursor we
    /// immediately populate the textarea.
    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) -> bool {
        self.history
            .on_entry_response(log_id, offset, entry, &mut self.textarea)
    }

    pub fn set_input_focus(&mut self, has_focus: bool) {
        self.update_border(has_focus);
    }

    /// Update the context-left percentage for display.
    pub fn set_context_left(&mut self, pct: f64) {
        self.context_left_percent = pct;
    }

    /// Handle a key event coming from the main UI.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        let result = match self.command_popup {
            Some(_) => self.handle_key_event_with_popup(key_event),
            None => self.handle_key_event_without_popup(key_event),
        };

        // Update (or hide/show) popup after processing the key.
        self.sync_command_popup();

        result
    }

    /// Handle key event when the slash-command popup is visible.
    fn handle_key_event_with_popup(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        let Some(popup) = self.command_popup.as_mut() else {
            tracing::error!("handle_key_event_with_popup called without an active popup");
            return (InputResult::None, false);
        };

        match key_event.into() {
            Input { key: Key::Up, .. } => {
                popup.move_up();
                (InputResult::None, true)
            }
            Input { key: Key::Down, .. } => {
                popup.move_down();
                (InputResult::None, true)
            }
            Input { key: Key::Tab, .. } => {
                if let Some(cmd) = popup.selected_command() {
                    let first_line = self
                        .textarea
                        .lines()
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    let starts_with_cmd = first_line
                        .trim_start()
                        .starts_with(&format!("/{}", cmd.command()));

                    if !starts_with_cmd {
                        self.textarea.select_all();
                        self.textarea.cut();
                        let _ = self.textarea.insert_str(format!("/{} ", cmd.command()));
                    }
                }
                (InputResult::None, true)
            }
            Input { key: Key::Enter, shift: false, alt: false, ctrl: false } => {
                if let Some(cmd) = popup.selected_command() {
                    self.textarea.select_all();
                    self.textarea.cut();
                    self.command_popup = None;
                    return (InputResult::None, true);
                }
                // Fallback to default newline handling if no command selected.
                self.handle_key_event_without_popup(key_event)
            }
            input => self.handle_input_basic(input),
        }
    }

    /// Handle key event when no popup is visible.
    fn handle_key_event_without_popup(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        let input: Input = key_event.into();
        match input {
            // -------------------------------------------------------------
            // History navigation (Up / Down) – only when the composer is not
            // empty or when the cursor is at the correct position, to avoid
            // interfering with normal cursor movement.
            // -------------------------------------------------------------
            Input { key: Key::Up, .. } => {
                if self.history.should_handle_navigation(&self.textarea) {
                    let consumed = self
                        .history
                        .navigate_up(&mut self.textarea, &self.app_event_tx);
                    if consumed {
                        return (InputResult::None, true);
                    }
                }
                self.handle_input_basic(input)
            }
            Input { key: Key::Down, .. } => {
                if self.history.should_handle_navigation(&self.textarea) {
                    let consumed = self
                        .history
                        .navigate_down(&mut self.textarea, &self.app_event_tx);
                    if consumed {
                        return (InputResult::None, true);
                    }
                }
                self.handle_input_basic(input)
            }
            Input {
                key: Key::Enter,
                shift: false,
                alt: false,
                ctrl: false,
            } => {
                let text = self.textarea.lines().join("\n");
                self.textarea.select_all();
                self.textarea.cut();

                if text.is_empty() {
                    (InputResult::None, true)
                } else {
                    self.history.record_local_submission(&text);
                    (InputResult::Submitted(text), true)
                }
            }
            Input { key: Key::Enter, .. }
            | Input { key: Key::Char('j'), ctrl: true, alt: false, shift: false } => {
                self.textarea.insert_newline();
                (InputResult::None, true)
            }
            Input { key: Key::Char('e'), ctrl: true, alt: false, shift: false } => {
                // Launch external editor for prompt drafting
                self.open_external_editor();
                (InputResult::None, true)
            }
            input => self.handle_input_basic(input),
        }
    }

    /// Handle generic Input events that modify the textarea content.
    fn handle_input_basic(&mut self, input: Input) -> (InputResult, bool) {
        self.textarea.input(input);
        (InputResult::None, true)
    }

    /// Launch an external editor on a temporary file pre-populated with the current draft,
    /// then reload the edited contents back into the textarea on exit.
    pub fn open_external_editor(&mut self) {
        use std::io::Write;
        use std::process::Command;
        // Dump current draft to a temp file
        let content = self.textarea.lines().join("\n");
        let mut tmp = match tempfile::NamedTempFile::new() {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("failed to create temp file for editor: {e}");
                return;
            }
        };
        if let Err(e) = write!(tmp, "{}", content) {
            tracing::error!("failed to write to temp file for editor: {e}");
            return;
        }
        let path = tmp.path();
        // Determine editor: VISUAL > EDITOR > nvim
        let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "nvim".into());
        // Launch editor and wait for exit
        if let Err(e) = Command::new(editor).arg(path).status() {
            tracing::error!("failed to launch editor: {e}");
            return;
        }
        // Read back edited contents (fall back to original on error)
        let new_text = std::fs::read_to_string(path).unwrap_or(content);
        // Replace textarea contents
        self.textarea.select_all();
        self.textarea.cut();
        let _ = self.textarea.insert_str(new_text);
    }

    /// Synchronize `self.command_popup` with the current text in the
    /// textarea. This must be called after every modification that can change
    /// the text so the popup is shown/updated/hidden as appropriate.
    fn sync_command_popup(&mut self) {
        // Inspect only the first line to decide whether to show the popup. In
        // the common case (no leading slash) we avoid copying the entire
        // textarea contents.
        let first_line = self
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");

        if first_line.starts_with('/') {
            // Create popup lazily when the user starts a slash command.
            let popup = self.command_popup.get_or_insert_with(CommandPopup::new);

            // Forward *only* the first line since `CommandPopup` only needs
            // the command token.
            popup.on_composer_text_change(first_line.to_string());
        } else if self.command_popup.is_some() {
            // Remove popup when '/' is no longer the first character.
            self.command_popup = None;
        }
    }

    pub fn calculate_required_height(&self, area: &Rect) -> u16 {
        let rows = self
            .textarea
            .lines()
            .len()
            .max(MIN_TEXTAREA_ROWS)
            .min(self.max_rows);
        let num_popup_rows = if let Some(popup) = &self.command_popup {
            popup.calculate_required_height(area)
        } else {
            0
        };

        // Include an extra row for the context-left indicator when not in popup mode
        let context_row = if self.command_popup.is_none() { 1 } else { 0 };
        rows as u16 + BORDER_LINES + num_popup_rows + context_row
    }

    fn update_border(&mut self, has_focus: bool) {
        struct BlockState {
            right_title: Line<'static>,
            border_style: Style,
        }

        let bs = if has_focus {
            BlockState {
                right_title: Line::from("Enter to send | Ctrl+D to quit | Ctrl+J for newline")
                    .alignment(Alignment::Right),
                border_style: Style::default(),
            }
        } else {
            BlockState {
                right_title: Line::from(""),
                border_style: Style::default().dim(),
            }
        };

        self.textarea.set_block(
            ratatui::widgets::Block::default()
                .title_bottom(bs.right_title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(bs.border_style),
        );
    }

    pub(crate) fn is_command_popup_visible(&self) -> bool {
        self.command_popup.is_some()
    }
}

impl WidgetRef for &ChatComposer<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        if let Some(popup) = &self.command_popup {
            let popup_height = popup.calculate_required_height(&area);

            // Split the provided rect so that the popup is rendered at the
            // *top* and the textarea occupies the remaining space below.
            let popup_rect = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: popup_height.min(area.height),
            };

            let textarea_rect = Rect {
                x: area.x,
                y: area.y + popup_rect.height,
                width: area.width,
                height: area.height.saturating_sub(popup_rect.height),
            };

            popup.render(popup_rect, buf);
            self.textarea.render(textarea_rect, buf);
        } else {
            self.textarea.render(area, buf);
        }
        // Render context-left indicator when not displaying a popup
        if self.command_popup.is_none() {
            let pct = self.context_left_percent.round();
            let text = format!("{:.0}% context left", pct);
            let color = if pct > 40.0 {
                Color::Green
            } else if pct > 25.0 {
                Color::Yellow
            } else {
                Color::Red
            };
            buf.set_string(area.x + 1, area.y + area.height - 1, text, Style::default().fg(color));
        }
    }
}
