//! A live status indicator that shows the *latest* log line emitted by the
//! application while the agent is processing a long‑running task.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

// We render the live text using markdown so it visually matches the history
// cells. Before rendering we strip any ANSI escape sequences to avoid writing
// raw control bytes into the back buffer.
use codex_ansi_escape::ansi_escape_line;

pub(crate) struct StatusIndicatorWidget {
    /// Latest text to display (truncated to the available width at render
    /// time).
    text: String,

    /// Animation state: reveal target `text` progressively like a typewriter.
    /// We compute the currently visible prefix length based on the current
    /// frame index and a constant typing speed.  The `base_frame` and
    /// `reveal_len_at_base` form the anchor from which we advance.
    last_target_len: usize,
    base_frame: usize,
    reveal_len_at_base: usize,

    frame_idx: Arc<AtomicUsize>,
    running: Arc<AtomicBool>,
    /// Ensure we only notify the app once per target text when the full
    /// reveal completes.
    completion_sent: AtomicBool,
    // Keep one sender alive to prevent the channel from closing while the
    // animation thread is still running. The field itself is currently not
    // accessed anywhere, therefore the leading underscore silences the
    // `dead_code` warning without affecting behavior.
    _app_event_tx: AppEventSender,
}

impl StatusIndicatorWidget {
    /// Create a new status indicator and start the animation timer.
    pub(crate) fn new(app_event_tx: AppEventSender) -> Self {
        let frame_idx = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));

        // Animation thread.
        {
            let frame_idx_clone = Arc::clone(&frame_idx);
            let running_clone = Arc::clone(&running);
            let app_event_tx_clone = app_event_tx.clone();
            thread::spawn(move || {
                let mut counter = 0usize;
                while running_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(33));
                    counter = counter.wrapping_add(1);
                    frame_idx_clone.store(counter, Ordering::Relaxed);
                    app_event_tx_clone.send(AppEvent::RequestRedraw);
                }
            });
        }

        Self {
            text: String::from("waiting for logs…"),
            last_target_len: 0,
            base_frame: 0,
            reveal_len_at_base: 0,
            frame_idx,
            running,
            completion_sent: AtomicBool::new(false),
            _app_event_tx: app_event_tx,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        // Compute wrapped height for the currently revealed text at the given width.
        let current_frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);
        let mut text = self.text.clone();
        // Only count what is currently revealed.
        let shown = self.current_shown_len(current_frame);
        if text.chars().count() > shown {
            let mut count = 0usize;
            let mut idx = text.len();
            for (i, _) in text.char_indices() {
                if count == shown {
                    idx = i;
                    break;
                }
                count += 1;
            }
            text.truncate(idx);
        }

        if width == 0 {
            return 1;
        }

        // Strip ANSI and hard-wrap to width (plain text).
        let sanitized = strip_ansi_all(&text);
        let wrapped = wrap_plain_text_to_width(&sanitized, width as usize);
        let mut h = wrapped.len() as u16;
        // Account for the blinking cursor potentially pushing the last line to the next row
        // when it's already at full width.
        if let Some(last) = wrapped.last() {
            let last_w: usize = last.spans.iter().map(|s| s.content.width()).sum();
            if last_w >= width as usize {
                h = h.saturating_add(1);
            }
        }
        // Reserve one extra row for the dot animation indicator.
        h = h.saturating_add(1);
        h.max(1)
    }

    /// Update the line that is displayed in the widget.
    pub(crate) fn update_text(&mut self, text: String) {
        // If the text hasn't changed, don't reset the baseline; let the
        // animation continue advancing naturally.
        if text == self.text {
            return;
        }
        // Update the target text, preserving newlines so wrapping matches history cells.
        // Strip ANSI escapes for the character count so the typewriter animation speed is stable.
        let stripped = {
            let line = ansi_escape_line(&text);
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        };
        let new_len = stripped.chars().count();

        // Compute how many characters are currently revealed so we can carry
        // this forward as the new baseline when target text changes.
        let current_frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);
        let shown_now = self.current_shown_len(current_frame);

        self.text = text;
        self.last_target_len = new_len;
        self.base_frame = current_frame;
        self.reveal_len_at_base = shown_now.min(new_len);
        self.completion_sent.store(false, Ordering::Relaxed);
    }

    /// Reset the animation and start revealing `text` from the beginning.
    pub(crate) fn restart_with_text(&mut self, text: String) {
        let sanitized = text.replace(['\n', '\r'], " ");
        let stripped = {
            let line = ansi_escape_line(&sanitized);
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        };

        let new_len = stripped.chars().count();
        let current_frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);

        self.text = sanitized;
        self.last_target_len = new_len;
        self.base_frame = current_frame;
        // Start from zero revealed characters for a fresh typewriter cycle.
        self.reveal_len_at_base = 0;
        self.completion_sent.store(false, Ordering::Relaxed);
    }

    /// Calculate how many characters should currently be visible given the
    /// animation baseline and frame counter.
    fn current_shown_len(&self, current_frame: usize) -> usize {
        const TYPING_CHARS_PER_FRAME: usize = 1;
        let frames = current_frame.saturating_sub(self.base_frame);
        let advanced = self
            .reveal_len_at_base
            .saturating_add(frames.saturating_mul(TYPING_CHARS_PER_FRAME));
        advanced.min(self.last_target_len)
    }
}

impl Drop for StatusIndicatorWidget {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.running.store(false, Ordering::Relaxed);
    }
}

impl WidgetRef for StatusIndicatorWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let widget_style = Style::default();
        // A subtle left border aligning visually with the input area.
        let block = Block::default()
            .padding(Padding::new(1, 0, 0, 0))
            .borders(Borders::LEFT)
            .border_type(BorderType::QuadrantOutside)
            .border_style(widget_style.dim());

        // Ensure we do not overflow width.
        let inner = block.inner(area);
        let inner_width = inner.width as usize;

        // Determine how many characters to reveal for the current frame and take
        // the corresponding prefix so we can render markdown for it.
        let shown =
            self.current_shown_len(self.frame_idx.load(std::sync::atomic::Ordering::Relaxed));
        let mut shown_text = self.text.clone();
        if shown_text.chars().count() > shown {
            let mut count = 0usize;
            let mut idx = shown_text.len();
            for (i, _) in shown_text.char_indices() {
                if count == shown {
                    idx = i;
                    break;
                }
                count += 1;
            }
            shown_text.truncate(idx);
        }

        // Strip ANSI and hard-wrap to width.
        let sanitized = strip_ansi_all(&shown_text);
        let mut lines: Vec<Line<'static>> = wrap_plain_text_to_width(&sanitized, inner_width);

        // Optional blinking cursor at the end of the last visual line.
        if let Some(last) = lines.last_mut() {
            let blink_on = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed) % 30 < 15;
            if blink_on {
                last.spans
                    .push(Span::styled("▋", Style::default().fg(Color::Cyan)));
            } else {
                last.spans.push(Span::raw(" "));
            }
        } else {
            lines.push(Line::from(Span::raw(" ")));
        }

        // Append a single-line dot animation below the text that grows and shrinks
        // in place according to the pattern (0,1,2,3,4,3,2,1,0). We map sizes to
        // increasingly bold/large dot glyphs so the dot appears to "breathe".
        const ANIM: [usize; 9] = [0, 1, 2, 3, 4, 3, 2, 1, 0];
        const DOTS: [&str; 5] = ["·", "•", "●", "◉", "⬤"]; // small → large
        const DOT_SLOWDOWN: usize = 6; // slow down animation relative to frame tick
        let frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);
        let idx = (frame / DOT_SLOWDOWN) % ANIM.len();
        let size = ANIM[idx];
        let glyph = DOTS[size];
        lines.push(Line::from(Span::styled(glyph, Style::default().fg(Color::Gray))));

        // If the animation for the current target has just finished, notify the app
        // so it can commit the cell to history and advance.
        {
            let current_frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);
            let shown = self.current_shown_len(current_frame);
            if self.last_target_len > 0
                && shown >= self.last_target_len
                && !self.completion_sent.swap(true, Ordering::Relaxed)
            {
                self._app_event_tx
                    .send(crate::app_event::AppEvent::LiveStatusRevealComplete);
            }
        }

        let paragraph = Paragraph::new(lines).block(block);
        paragraph.render_ref(area, buf);
    }
}

/// Strip ANSI escapes from a multi-line string.
fn strip_ansi_all(s: &str) -> String {
    s.split('\n')
        .map(|line| {
            let l = ansi_escape_line(line);
            l.spans
                .iter()
                .map(|sp| sp.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hard-wrap plain text to a given terminal width using display cells.
fn wrap_plain_text_to_width(s: &str, width: usize) -> Vec<Line<'static>> {
    let w = width.max(1);
    let mut out: Vec<Line<'static>> = Vec::new();
    for raw_line in s.split('\n') {
        if raw_line.is_empty() {
            out.push(Line::from(String::new()));
            continue;
        }
        let mut remaining = raw_line;
        while !remaining.is_empty() {
            let (prefix, suffix, taken_w) = take_prefix_by_width(remaining, w);
            out.push(Line::from(Span::raw(prefix)));
            if taken_w >= remaining.width() {
                break;
            }
            remaining = suffix;
        }
    }
    if out.is_empty() {
        out.push(Line::from(String::new()));
    }
    out
}

/// Take a prefix of `s` whose display width is at most `max_cols` terminal cells.
/// Returns (prefix, suffix, prefix_width).
fn take_prefix_by_width(s: &str, max_cols: usize) -> (String, &str, usize) {
    if max_cols == 0 || s.is_empty() {
        return (String::new(), s, 0);
    }

    let mut cols = 0usize;
    let mut end_idx = 0usize;
    for (i, ch) in s.char_indices() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cols.saturating_add(ch_width) > max_cols {
            break;
        }
        cols += ch_width;
        end_idx = i + ch.len_utf8();
    }

    let prefix = s[..end_idx].to_string();
    let suffix = &s[end_idx..];
    (prefix, suffix, cols)
}
