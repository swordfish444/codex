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
            text: String::from("waiting for model"),
            last_target_len: 0,
            base_frame: 0,
            reveal_len_at_base: 0,
            frame_idx,
            running,
            completion_sent: AtomicBool::new(false),
            _app_event_tx: app_event_tx,
        }
    }

    pub fn desired_height(&self, _width: u16) -> u16 { 1 }

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
        // Increase typewriter speed (~5x): reveal more characters per frame.
        const TYPING_CHARS_PER_FRAME: usize = 7;
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
        // Ensure minimal height
        if area.height == 0 || area.width == 0 { return; }
        // Plain rendering: no borders or padding so the live cell is visually
        // indistinguishable from terminal scrollback. No left bar.
        let inner_width = area.width as usize;

        // Compose a single status line like: "▌ Working [·] waiting for model"
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("▌ ", Style::default().fg(Color::Cyan)));
        spans.push(Span::raw("Working "));

        // Append animated dot in brackets.
        const ANIM: [usize; 9] = [0, 1, 2, 3, 4, 3, 2, 1, 0];
        const DOTS: [&str; 5] = ["·", "•", "●", "◉", "⬤"]; // small → large
        const DOT_SLOWDOWN: usize = 6; // slow down animation relative to frame tick
        let frame = self.frame_idx.load(std::sync::atomic::Ordering::Relaxed);
        let idx = (frame / DOT_SLOWDOWN) % ANIM.len();
        let size = ANIM[idx];
        let glyph = DOTS[size];
        spans.push(Span::raw("["));
        spans.push(Span::styled(glyph, Style::default().fg(Color::Gray)));
        spans.push(Span::raw("] "));
        spans.push(Span::raw(self.text.clone()));

        // Truncate spans to fit the width.
        let mut acc: Vec<Span<'static>> = Vec::new();
        let mut used = 0usize;
        for s in spans {
            let w = s.content.width();
            if used + w <= inner_width { acc.push(s); used += w; } else { break; }
        }
        let lines = vec![Line::from(acc)];

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

        let paragraph = Paragraph::new(lines);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use std::sync::mpsc::channel;

    #[test]
    fn renders_without_left_border_or_padding() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(tx);
        w.restart_with_text("Hello".to_string());

        let area = ratatui::layout::Rect::new(0, 0, 30, 1);
        // Allow a short delay so the typewriter reveals the first character.
        std::thread::sleep(std::time::Duration::from_millis(120));
        let mut buf = ratatui::buffer::Buffer::empty(area);
        w.render_ref(area, &mut buf);

        // Leftmost column has the left bar 
        let ch0 = buf[(0, 0)].symbol().chars().next().unwrap_or(' ');
        assert_eq!(ch0, '▌', "expected left bar at col 0: {ch0:?}");
    }

    #[test]
    fn bracket_dot_animation_is_present_on_last_line() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(tx);
        w.restart_with_text("Hi".to_string());
        // Ensure some frames elapse so we get a stable state.
        std::thread::sleep(std::time::Duration::from_millis(120));

        let area = ratatui::layout::Rect::new(0, 0, 30, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        w.render_ref(area, &mut buf);

        // Single line; it should contain "Working [" and closing "]" and the provided text.
        let mut row = String::new();
        for x in 0..area.width { row.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' ')); }
        assert!(row.contains("Working ["), "expected status prefix: {row:?}");
        assert!(row.contains("]"), "expected bracket: {row:?}");
        assert!(row.contains("Hi"), "expected provided text in status: {row:?}");
    }
}
