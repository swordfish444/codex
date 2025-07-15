use crate::cell_widget::CellWidget;
use crate::history_cell::CommandOutput;
use crate::history_cell::HistoryCell;
use crate::history_cell::PatchEventType;
use codex_core::config::Config;
use codex_core::protocol::FileChange;
use codex_core::protocol::SessionConfiguredEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::*;
use serde_json::Value as JsonValue;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

/// A single history entry plus its cached wrapped-line count.
struct Entry {
    cell: HistoryCell,
    line_count: Cell<usize>,
}

pub struct ConversationHistoryWidget {
    entries: Vec<Entry>,
    /// The width (in terminal cells/columns) that [`Entry::line_count`] was
    /// computed for. When the available width changes we recompute counts.
    cached_width: Cell<u16>,
    scroll_position: usize,
    /// Number of lines the last time render_ref() was called
    num_rendered_lines: Cell<usize>,
    /// The height of the viewport last time render_ref() was called
    last_viewport_height: Cell<usize>,
    has_input_focus: bool,
    /// Scratch buffer used while incrementally streaming an agent message so we can re-render markdown at newline boundaries.
    streaming_agent_message_buf: String,
    /// Scratch buffer used while incrementally streaming agent reasoning so we can re-render markdown at newline boundaries.
    streaming_agent_reasoning_buf: String,
}

impl ConversationHistoryWidget {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cached_width: Cell::new(0),
            scroll_position: usize::MAX,
            num_rendered_lines: Cell::new(0),
            last_viewport_height: Cell::new(0),
            has_input_focus: false,
            streaming_agent_message_buf: String::new(),
            streaming_agent_reasoning_buf: String::new(),
        }
    }

    pub(crate) fn set_input_focus(&mut self, has_input_focus: bool) {
        self.has_input_focus = has_input_focus;
    }

    /// Returns true if it needs a redraw.
    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down(1);
                true
            }
            KeyCode::PageUp | KeyCode::Char('b') => {
                self.scroll_page_up();
                true
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll_page_down();
                true
            }
            _ => false,
        }
    }

    /// Negative delta scrolls up; positive delta scrolls down.
    pub(crate) fn scroll(&mut self, delta: i32) {
        match delta.cmp(&0) {
            std::cmp::Ordering::Less => self.scroll_up(-delta as u32),
            std::cmp::Ordering::Greater => self.scroll_down(delta as u32),
            std::cmp::Ordering::Equal => {}
        }
    }

    fn scroll_up(&mut self, num_lines: u32) {
        // Convert sticky-to-bottom sentinel into a concrete offset anchored at the bottom.
        if self.scroll_position == usize::MAX {
            self.scroll_position = sticky_offset(
                self.num_rendered_lines.get(),
                self.last_viewport_height.get(),
            );
        }
        self.scroll_position = self.scroll_position.saturating_sub(num_lines as usize);
    }

    fn scroll_down(&mut self, num_lines: u32) {
        // Nothing to do if we're already pinned to the bottom.
        if self.scroll_position == usize::MAX {
            return;
        }
        let viewport_height = self.last_viewport_height.get().max(1);
        let max_scroll = sticky_offset(self.num_rendered_lines.get(), viewport_height);
        let new_pos = self.scroll_position.saturating_add(num_lines as usize);
        if new_pos >= max_scroll {
            // Switch to sticky-bottom mode so subsequent output pins view.
            self.scroll_position = usize::MAX;
        } else {
            self.scroll_position = new_pos;
        }
    }

    /// Scroll up by one full viewport height (Page Up).
    fn scroll_page_up(&mut self) {
        let viewport_height = self.last_viewport_height.get().max(1);
        if self.scroll_position == usize::MAX {
            self.scroll_position = sticky_offset(self.num_rendered_lines.get(), viewport_height);
        }
        self.scroll_position = self.scroll_position.saturating_sub(viewport_height);
    }

    /// Scroll down by one full viewport height (Page Down).
    fn scroll_page_down(&mut self) {
        if self.scroll_position == usize::MAX {
            return;
        }
        let viewport_height = self.last_viewport_height.get().max(1);
        let max_scroll = sticky_offset(self.num_rendered_lines.get(), viewport_height);
        let new_pos = self.scroll_position.saturating_add(viewport_height);
        if new_pos >= max_scroll {
            self.scroll_position = usize::MAX;
        } else {
            self.scroll_position = new_pos;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_position = usize::MAX;
    }

    /// Note `model` could differ from `config.model` if the agent decided to
    /// use a different model than the one requested by the user.
    pub fn add_session_info(&mut self, config: &Config, event: SessionConfiguredEvent) {
        // In practice, SessionConfiguredEvent should always be the first entry
        // in the history, but it is possible that an error could be sent
        // before the session info.
        let has_welcome_message = self
            .entries
            .iter()
            .any(|entry| matches!(entry.cell, HistoryCell::WelcomeMessage { .. }));
        self.add_to_history(HistoryCell::new_session_info(
            config,
            event,
            !has_welcome_message,
        ));
    }

    pub fn add_user_message(&mut self, message: String) {
        self.add_to_history(HistoryCell::new_user_prompt(message));
    }

    pub fn add_agent_message(&mut self, config: &Config, message: String) {
        // Reset streaming scratch because we are starting a fresh agent message.
        self.streaming_agent_message_buf.clear();
        self.streaming_agent_message_buf.push_str(&message);
        self.add_to_history(HistoryCell::new_agent_message(config, message));
    }

    pub fn add_agent_reasoning(&mut self, config: &Config, text: String) {
        self.streaming_agent_reasoning_buf.clear();
        self.streaming_agent_reasoning_buf.push_str(&text);
        self.add_to_history(HistoryCell::new_agent_reasoning(config, text));
    }

    /// Append incremental assistant text.
    ///
    /// Previous heuristic: fast‑append chunks until we saw a newline, then re‑render.
    /// This caused visible "one‑word" lines (e.g., "The" -> "The user") when models
    /// streamed small token fragments and also delayed Markdown styling (headings, code fences)
    /// until the first newline arrived.  To improve perceived quality we now *always* re‑render
    /// the accumulated markdown buffer on every incoming delta chunk.  We still apply the
    /// soft‑break collapsing heuristic (outside fenced code blocks) so interim layout more closely
    /// matches the final message and reduces layout thrash.
    pub fn append_agent_message_delta(&mut self, _config: &Config, text: String) {
        if text.is_empty() {
            return;
        }
        // Accumulate full buffer.
        self.streaming_agent_message_buf.push_str(&text);

        let collapsed = collapse_single_newlines_for_streaming(&self.streaming_agent_message_buf);
        if let Some(idx) = last_agent_message_idx(&self.entries) {
            let width = self.cached_width.get();
            let entry = &mut self.entries[idx];
            entry.cell = HistoryCell::new_agent_message(_config, collapsed);
            // Drop trailing blank so we can continue streaming additional tokens cleanly.
            if let HistoryCell::AgentMessage { view } = &mut entry.cell {
                drop_trailing_blank_line(&mut view.lines);
            }
            if width > 0 {
                update_entry_height(entry, width);
            }
        } else {
            // No existing cell? Start a new one.
            self.add_agent_message(_config, self.streaming_agent_message_buf.clone());
        }
    }

    /// Append incremental reasoning text (mirrors `append_agent_message_delta`).
    pub fn append_agent_reasoning_delta(&mut self, _config: &Config, text: String) {
        if text.is_empty() {
            return;
        }
        self.streaming_agent_reasoning_buf.push_str(&text);

        let collapsed = collapse_single_newlines_for_streaming(&self.streaming_agent_reasoning_buf);
        if let Some(idx) = last_agent_reasoning_idx(&self.entries) {
            let width = self.cached_width.get();
            let entry = &mut self.entries[idx];
            entry.cell = HistoryCell::new_agent_reasoning(_config, collapsed);
            if let HistoryCell::AgentReasoning { view } = &mut entry.cell {
                drop_trailing_blank_line(&mut view.lines);
            }
            if width > 0 {
                update_entry_height(entry, width);
            }
        } else {
            self.add_agent_reasoning(_config, self.streaming_agent_reasoning_buf.clone());
        }
    }

    /// Replace the most recent AgentMessage cell with the fully accumulated `text`.
    /// This should be called once the turn is complete so we can render proper markdown.
    pub fn replace_last_agent_message(&mut self, config: &Config, text: String) {
        self.streaming_agent_message_buf.clear();
        if let Some(idx) = last_agent_message_idx(&self.entries) {
            let width = self.cached_width.get();
            let entry = &mut self.entries[idx];
            entry.cell = HistoryCell::new_agent_message(config, text);
            if width > 0 {
                update_entry_height(entry, width);
            }
        } else {
            // No existing AgentMessage (shouldn't happen) – append new.
            self.add_agent_message(config, text);
        }
    }

    /// Replace the most recent AgentReasoning cell with the fully accumulated `text`.
    pub fn replace_last_agent_reasoning(&mut self, config: &Config, text: String) {
        self.streaming_agent_reasoning_buf.clear();
        if let Some(idx) = last_agent_reasoning_idx(&self.entries) {
            let width = self.cached_width.get();
            let entry = &mut self.entries[idx];
            entry.cell = HistoryCell::new_agent_reasoning(config, text);
            if width > 0 {
                update_entry_height(entry, width);
            }
        } else {
            self.add_agent_reasoning(config, text);
        }
    }

    pub fn add_background_event(&mut self, message: String) {
        self.add_to_history(HistoryCell::new_background_event(message));
    }

    pub fn add_diff_output(&mut self, diff_output: String) {
        self.add_to_history(HistoryCell::new_diff_output(diff_output));
    }

    pub fn add_error(&mut self, message: String) {
        self.add_to_history(HistoryCell::new_error_event(message));
    }

    /// Add a pending patch entry (before user approval).
    pub fn add_patch_event(
        &mut self,
        event_type: PatchEventType,
        changes: HashMap<PathBuf, FileChange>,
    ) {
        self.add_to_history(HistoryCell::new_patch_event(event_type, changes));
    }

    pub fn add_active_exec_command(&mut self, call_id: String, command: Vec<String>) {
        self.add_to_history(HistoryCell::new_active_exec_command(call_id, command));
    }

    pub fn add_active_mcp_tool_call(
        &mut self,
        call_id: String,
        server: String,
        tool: String,
        arguments: Option<JsonValue>,
    ) {
        self.add_to_history(HistoryCell::new_active_mcp_tool_call(
            call_id, server, tool, arguments,
        ));
    }

    fn add_to_history(&mut self, cell: HistoryCell) {
        let width = self.cached_width.get();
        let count = if width > 0 { cell.height(width) } else { 0 };

        self.entries.push(Entry {
            cell,
            line_count: Cell::new(count),
        });
    }

    pub fn record_completed_exec_command(
        &mut self,
        call_id: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    ) {
        let width = self.cached_width.get();
        for entry in self.entries.iter_mut() {
            let cell = &mut entry.cell;
            if let HistoryCell::ActiveExecCommand {
                call_id: history_id,
                command,
                start,
                ..
            } = cell
            {
                if &call_id == history_id {
                    *cell = HistoryCell::new_completed_exec_command(
                        command.clone(),
                        CommandOutput {
                            exit_code,
                            stdout,
                            stderr,
                            duration: start.elapsed(),
                        },
                    );

                    // Update cached line count.
                    if width > 0 {
                        update_entry_height(entry, width);
                    }
                    break;
                }
            }
        }
    }

    pub fn record_completed_mcp_tool_call(
        &mut self,
        call_id: String,
        success: bool,
        result: Result<mcp_types::CallToolResult, String>,
    ) {
        let width = self.cached_width.get();
        for entry in self.entries.iter_mut() {
            if let HistoryCell::ActiveMcpToolCall {
                call_id: history_id,
                invocation,
                start,
                ..
            } = &entry.cell
            {
                if &call_id == history_id {
                    let completed = HistoryCell::new_completed_mcp_tool_call(
                        width,
                        invocation.clone(),
                        *start,
                        success,
                        result,
                    );
                    entry.cell = completed;

                    if width > 0 {
                        update_entry_height(entry, width);
                    }

                    break;
                }
            }
        }
    }
}

impl WidgetRef for ConversationHistoryWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let (title, border_style) = if self.has_input_focus {
            (
                "Messages (↑/↓ or j/k = line,  b/space = page)",
                Style::default().fg(Color::LightYellow),
            )
        } else {
            ("Messages (tab to focus)", Style::default().dim())
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        // Compute the inner area that will be available for the list after
        // the surrounding `Block` is drawn.
        let inner = block.inner(area);
        let viewport_height = inner.height as usize;

        // Cache (and if necessary recalculate) the wrapped line counts for every
        // [`HistoryCell`] so that our scrolling math accounts for text
        // wrapping.  We always reserve one column on the right-hand side for the
        // scrollbar so that the content never renders "under" the scrollbar.
        let effective_width = inner.width.saturating_sub(1);

        if effective_width == 0 {
            return; // Nothing to draw – avoid division by zero.
        }

        // Recompute cache if the effective width changed.
        let num_lines: usize = if self.cached_width.get() != effective_width {
            self.cached_width.set(effective_width);

            let mut num_lines: usize = 0;
            for entry in &self.entries {
                let count = entry.cell.height(effective_width);
                num_lines += count;
                entry.line_count.set(count);
            }
            num_lines
        } else {
            self.entries.iter().map(|e| e.line_count.get()).sum()
        };

        // Determine the scroll position (respect sticky-to-bottom sentinel and clamp).
        let max_scroll = sticky_offset(num_lines, viewport_height);
        let scroll_pos = if self.scroll_position == usize::MAX {
            max_scroll
        } else {
            clamp_scroll_pos(self.scroll_position, max_scroll)
        };

        // ------------------------------------------------------------------
        // Render order:
        //   1. Clear full widget area (avoid artifacts from prior frame).
        //   2. Draw the surrounding Block (border and title).
        //   3. Render *each* visible HistoryCell into its own sub-Rect while
        //      respecting partial visibility at the top and bottom.
        //   4. Draw the scrollbar track / thumb in the reserved column.
        // ------------------------------------------------------------------

        // Clear entire widget area first.
        Clear.render(area, buf);

        // Draw border + title.
        block.render(area, buf);

        // ------------------------------------------------------------------
        // Calculate which cells are visible for the current scroll position
        // and paint them one by one.
        // ------------------------------------------------------------------

        let mut y_cursor = inner.y; // first line inside viewport
        let mut remaining_height = inner.height as usize;
        let mut lines_to_skip = scroll_pos; // number of wrapped lines to skip (above viewport)

        for entry in &self.entries {
            let cell_height = entry.line_count.get();

            // Completely above viewport? Skip whole cell.
            if lines_to_skip >= cell_height {
                lines_to_skip -= cell_height;
                continue;
            }

            // Determine how much of this cell is visible.
            let visible_height = (cell_height - lines_to_skip).min(remaining_height);

            if visible_height == 0 {
                break; // no space left
            }

            let cell_rect = Rect {
                x: inner.x,
                y: y_cursor,
                width: effective_width,
                height: visible_height as u16,
            };

            entry.cell.render_window(lines_to_skip, cell_rect, buf);

            // Advance cursor inside viewport.
            y_cursor += visible_height as u16;
            remaining_height -= visible_height;

            // After the first (possibly partially skipped) cell, we no longer
            // need to skip lines at the top.
            lines_to_skip = 0;

            if remaining_height == 0 {
                break; // viewport filled
            }
        }

        // Always render a scrollbar *track* so the reserved column is filled.
        let overflow = num_lines.saturating_sub(viewport_height);

        let mut scroll_state = ScrollbarState::default()
            // The Scrollbar widget expects the *content* height minus the
            // viewport height.  When there is no overflow we still provide 0
            // so that the widget renders only the track without a thumb.
            .content_length(overflow)
            .position(scroll_pos);

        {
            // Choose a thumb color that stands out only when this pane has focus so that the
            // user's attention is naturally drawn to the active viewport. When unfocused we show
            // a low-contrast thumb so the scrollbar fades into the background without becoming
            // invisible.
            let thumb_style = if self.has_input_focus {
                Style::reset().fg(Color::LightYellow)
            } else {
                Style::reset().fg(Color::Gray)
            };

            // By default the Scrollbar widget inherits any style that was
            // present in the underlying buffer cells. That means if a colored
            // line happens to be underneath the scrollbar, the track (and
            // potentially the thumb) adopt that color. Explicitly setting the
            // track/thumb styles ensures we always draw the scrollbar with a
            // consistent palette regardless of what content is behind it.
            StatefulWidget::render(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"))
                    .begin_style(Style::reset().fg(Color::DarkGray))
                    .end_style(Style::reset().fg(Color::DarkGray))
                    .thumb_symbol("█")
                    .thumb_style(thumb_style)
                    .track_symbol(Some("│"))
                    .track_style(Style::reset().fg(Color::DarkGray)),
                inner,
                buf,
                &mut scroll_state,
            );
        }

        // Update auxiliary stats that the scroll handlers rely on.
        self.num_rendered_lines.set(num_lines);
        self.last_viewport_height.set(viewport_height);
    }
}

/// Common [`Wrap`] configuration used for both measurement and rendering so
/// they stay in sync.
#[inline]
pub(crate) const fn wrap_cfg() -> ratatui::widgets::Wrap {
    ratatui::widgets::Wrap { trim: false }
}

// ---------------------------------------------------------------------------
// Scrolling helpers (private)
// ---------------------------------------------------------------------------
#[inline]
fn sticky_offset(num_lines: usize, viewport_height: usize) -> usize {
    num_lines.saturating_sub(viewport_height.max(1))
}

#[inline]
fn clamp_scroll_pos(pos: usize, max_scroll: usize) -> usize {
    pos.min(max_scroll)
}

// ---------------------------------------------------------------------------
// Streaming helpers (private)
// ---------------------------------------------------------------------------

/// Locate the most recent `HistoryCell::AgentMessage` entry.
fn last_agent_message_idx(entries: &[Entry]) -> Option<usize> {
    entries
        .iter()
        .rposition(|e| matches!(e.cell, HistoryCell::AgentMessage { .. }))
}

/// Locate the most recent `HistoryCell::AgentReasoning` entry.
fn last_agent_reasoning_idx(entries: &[Entry]) -> Option<usize> {
    entries
        .iter()
        .rposition(|e| matches!(e.cell, HistoryCell::AgentReasoning { .. }))
}

/// True if the line is an empty spacer (single empty span).
fn is_blank_line(line: &Line<'_>) -> bool {
    line.spans.len() == 1 && line.spans[0].content.is_empty()
}

/// Ensure that the vector has *at least* one body line after the header.
/// A freshly-created AgentMessage/Reasoning cell always has a header + blank line,
/// but streaming cells may be created empty; this makes sure we have a target line.
#[allow(dead_code)]
fn ensure_body_line(lines: &mut Vec<Line<'static>>) {
    if lines.len() < 2 {
        lines.push(Line::from(""));
    }
}

/// Trim a single trailing blank spacer line (but preserve intentional paragraph breaks).
fn drop_trailing_blank_line(lines: &mut Vec<Line<'static>>) {
    if let Some(last) = lines.last() {
        if is_blank_line(last) {
            lines.pop();
        }
    }
}

/// Append streaming text, honouring embedded newlines.
#[allow(dead_code)]
fn append_streaming_text_chunks(lines: &mut Vec<Line<'static>>, text: &str) {
    // NOTE: This helper is now a fallback path only (we eagerly re-render accumulated markdown).
    // Still, keep behaviour sane: drop trailing spacer, ensure a writable body line, then append.
    drop_trailing_blank_line(lines);
    ensure_body_line(lines);
    if let Some(last_line) = lines.last_mut() {
        last_line.spans.push(Span::raw(text.to_string()));
    } else {
        lines.push(Line::from(text.to_string()));
    }
}

/// Re-measure a mutated entry at `width` columns and update its cached height.
fn update_entry_height(entry: &Entry, width: u16) {
    entry.line_count.set(entry.cell.height(width));
}

/// Collapse *single* newlines in a streaming buffer into spaces so that interim streaming
/// renders more closely match final Markdown layout — *except* when we detect fenced code blocks.
/// If the accumulated text contains a Markdown code fence (``` or ~~~), we preserve **all**
/// newlines verbatim so multi-line code renders correctly while streaming.
fn collapse_single_newlines_for_streaming(src: &str) -> String {
    // Quick fence detection. If we see a code fence marker anywhere in the accumulated text,
    // skip collapsing entirely so we do not mangle code formatting.
    if src.contains("```") || src.contains("~~~") {
        return src.to_string();
    }

    let mut out = String::with_capacity(src.len());
    let mut pending_newlines = 0usize;
    for ch in src.chars() {
        if ch == '\n' {
            pending_newlines += 1;
            continue;
        }
        if pending_newlines == 1 {
            // soft break -> space
            out.push(' ');
        } else if pending_newlines > 1 {
            // preserve paragraph breaks exactly
            for _ in 0..pending_newlines {
                out.push('\n');
            }
        }
        pending_newlines = 0;
        out.push(ch);
    }
    // flush tail
    if pending_newlines == 1 {
        out.push(' ');
    } else if pending_newlines > 1 {
        for _ in 0..pending_newlines {
            out.push('\n');
        }
    }
    out
}
