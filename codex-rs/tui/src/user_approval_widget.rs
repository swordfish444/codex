//! A modal widget that prompts the user to approve or deny an action
//! requested by the agent.
//!
//! This is a (very) rough port of
//! `src/components/chat/terminal-chat-command-review.tsx` from the TypeScript
//! UI to Rust using [`ratatui`]. The goal is feature‑parity for the keyboard
//! driven workflow – a fully‑fledged visual match is not required.

use std::path::PathBuf;
use std::sync::LazyLock;

use codex_core::protocol::Op;
use codex_core::protocol::ReviewDecision;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::history_cell;
use crate::render::line_utils::prefix_lines;
use crate::selection_menu::GenericDisplayRow;
use crate::selection_menu::ScrollState;
use crate::selection_menu::render_rows;
use crate::text_formatting::truncate_text;

/// Request coming from the agent that needs user approval.
pub(crate) enum ApprovalRequest {
    Exec {
        id: String,
        command: Vec<String>,
        reason: Option<String>,
    },
    ApplyPatch {
        id: String,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    },
}

/// Options displayed in the *select* mode.
///
/// The `key` is matched case-insensitively.
struct SelectOption {
    label: &'static str,
    description: &'static str,
    key: KeyCode,
    decision: ReviewDecision,
}

static COMMAND_SELECT_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    vec![
        SelectOption {
            label: "Yes",
            description: "Approve and run the command",
            key: KeyCode::Char('y'),
            decision: ReviewDecision::Approved,
        },
        SelectOption {
            label: "Always",
            description: "Approve the command for the remainder of this session",
            key: KeyCode::Char('a'),
            decision: ReviewDecision::ApprovedForSession,
        },
        SelectOption {
            label: "No, provide feedback",
            description: "Do not run the command; provide feedback",
            key: KeyCode::Char('n'),
            decision: ReviewDecision::Abort,
        },
    ]
});

static PATCH_SELECT_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    vec![
        SelectOption {
            label: "Yes",
            description: "Approve and apply the changes",
            key: KeyCode::Char('y'),
            decision: ReviewDecision::Approved,
        },
        SelectOption {
            label: "No, provide feedback",
            description: "Do not apply the changes; provide feedback",
            key: KeyCode::Char('n'),
            decision: ReviewDecision::Abort,
        },
    ]
});

/// A modal prompting the user to approve or deny the pending request.
pub(crate) struct UserApprovalWidget {
    approval_request: ApprovalRequest,
    app_event_tx: AppEventSender,
    confirmation_prompt: Paragraph<'static>,
    select_options: &'static Vec<SelectOption>,
    scroll_state: ScrollState,

    /// Set to `true` once a decision has been sent – the parent view can then
    /// remove this widget from its queue.
    done: bool,
}

impl UserApprovalWidget {
    pub(crate) fn new(approval_request: ApprovalRequest, app_event_tx: AppEventSender) -> Self {
        let confirmation_prompt = match &approval_request {
            ApprovalRequest::Exec { reason, .. } => {
                let mut contents: Vec<Line<'static>> = vec![];
                if let Some(reason) = reason {
                    contents.push(Line::from(reason.clone().italic()));
                    contents.push(Line::from(""));
                }
                if contents.is_empty() {
                    contents.push(Line::from(""));
                }
                let prefixed = prefix_lines(contents, "▌ ".dim(), "▌ ".dim());
                Paragraph::new(prefixed).wrap(Wrap { trim: false })
            }
            ApprovalRequest::ApplyPatch {
                reason, grant_root, ..
            } => {
                let mut contents: Vec<Line<'static>> = vec![];

                if let Some(r) = reason {
                    contents.push(Line::from(r.clone().italic()));
                    contents.push(Line::from(""));
                }

                if let Some(root) = grant_root {
                    contents.push(Line::from(format!(
                        "This will grant write access to {} for the remainder of this session.",
                        root.display()
                    )));
                    contents.push(Line::from(""));
                }

                if contents.is_empty() {
                    contents.push(Line::from(""));
                }

                let prefixed = prefix_lines(contents, "▌ ".dim(), "▌ ".dim());
                Paragraph::new(prefixed).wrap(Wrap { trim: false })
            }
        };

        let select_options = match &approval_request {
            ApprovalRequest::Exec { .. } => &COMMAND_SELECT_OPTIONS,
            ApprovalRequest::ApplyPatch { .. } => &PATCH_SELECT_OPTIONS,
        };

        let mut scroll_state = ScrollState::new();
        let len = select_options.len();
        scroll_state.clamp_selection(len);
        scroll_state.ensure_visible(len, len);

        Self {
            select_options,
            approval_request,
            app_event_tx,
            confirmation_prompt,
            scroll_state,
            done: false,
        }
    }

    fn get_confirmation_prompt_height(&self, width: u16) -> u16 {
        // Should cache this for last value of width.
        self.confirmation_prompt.line_count(width) as u16
    }

    /// Process a `KeyEvent` coming from crossterm. Always consumes the event
    /// while the modal is visible.
    /// Process a key event originating from crossterm. As the modal fully
    /// captures input while visible, we don’t need to report whether the event
    /// was consumed—callers can assume it always is.
    pub(crate) fn handle_key_event(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Press {
            self.handle_select_key(key);
        }
    }

    /// Normalize a key for comparison.
    /// - For `KeyCode::Char`, converts to lowercase for case-insensitive matching.
    /// - Other key codes are returned unchanged.
    fn normalize_keycode(code: KeyCode) -> KeyCode {
        match code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        }
    }

    fn selected_option(&self) -> Option<&SelectOption> {
        self.scroll_state
            .selected_idx
            .and_then(|idx| self.select_options.get(idx))
    }

    fn format_option_label(option: &SelectOption) -> String {
        match option.key {
            KeyCode::Char(c) => format!("[{}] {}", c.to_ascii_uppercase(), option.label),
            KeyCode::Enter => format!("[Enter] {}", option.label),
            KeyCode::Esc => format!("[Esc] {}", option.label),
            KeyCode::Tab => format!("[Tab] {}", option.label),
            KeyCode::BackTab => format!("[Shift-Tab] {}", option.label),
            KeyCode::F(n) => format!("[F{n}] {}", option.label),
            _ => option.label.to_string(),
        }
    }

    /// Handle Ctrl-C pressed by the user while the modal is visible.
    /// Behaves like pressing Escape: abort the request and close the modal.
    pub(crate) fn on_ctrl_c(&mut self) {
        self.send_decision(ReviewDecision::Abort);
    }

    fn handle_select_key(&mut self, key_event: KeyEvent) {
        let len = self.select_options.len();
        if len == 0 {
            return;
        }

        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                self.scroll_state.move_up_wrap(len);
                self.scroll_state.ensure_visible(len, len);
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                self.scroll_state.move_down_wrap(len);
                self.scroll_state.ensure_visible(len, len);
            }
            KeyCode::Enter => {
                if let Some(opt) = self.selected_option() {
                    self.send_decision(opt.decision);
                }
            }
            KeyCode::Esc => {
                self.send_decision(ReviewDecision::Abort);
            }
            other => {
                let normalized = Self::normalize_keycode(other);
                if let Some((idx, opt)) = self
                    .select_options
                    .iter()
                    .enumerate()
                    .find(|(_, opt)| Self::normalize_keycode(opt.key) == normalized)
                {
                    self.scroll_state.selected_idx = Some(idx);
                    self.scroll_state.ensure_visible(len, len);
                    self.send_decision(opt.decision);
                }
            }
        }
    }

    fn send_decision(&mut self, decision: ReviewDecision) {
        self.send_decision_with_feedback(decision, String::new())
    }

    fn send_decision_with_feedback(&mut self, decision: ReviewDecision, feedback: String) {
        match &self.approval_request {
            ApprovalRequest::Exec { command, .. } => {
                let full_cmd = strip_bash_lc_and_escape(command);
                // Construct a concise, single-line summary of the command:
                // - If multi-line, take the first line and append " ...".
                // - Truncate to 80 graphemes.
                let mut snippet = match full_cmd.split_once('\n') {
                    Some((first, _)) => format!("{first} ..."),
                    None => full_cmd.clone(),
                };
                // Enforce the 80 character length limit.
                snippet = truncate_text(&snippet, 80);

                let mut result_spans: Vec<Span<'static>> = Vec::new();
                match decision {
                    ReviewDecision::Approved => {
                        result_spans.extend(vec![
                            "✔ ".fg(Color::Green),
                            "You ".into(),
                            "approved".bold(),
                            " codex to run ".into(),
                            snippet.clone().dim(),
                            " this time".bold(),
                        ]);
                    }
                    ReviewDecision::ApprovedForSession => {
                        result_spans.extend(vec![
                            "✔ ".fg(Color::Green),
                            "You ".into(),
                            "approved".bold(),
                            " codex to run ".into(),
                            snippet.clone().dim(),
                            " every time this session".bold(),
                        ]);
                    }
                    ReviewDecision::Denied => {
                        result_spans.extend(vec![
                            "✗ ".fg(Color::Red),
                            "You ".into(),
                            "did not approve".bold(),
                            " codex to run ".into(),
                            snippet.clone().dim(),
                        ]);
                    }
                    ReviewDecision::Abort => {
                        result_spans.extend(vec![
                            "✗ ".fg(Color::Red),
                            "You ".into(),
                            "canceled".bold(),
                            " the request to run ".into(),
                            snippet.clone().dim(),
                        ]);
                    }
                }

                let mut lines: Vec<Line<'static>> = vec![Line::from(result_spans)];

                if !feedback.trim().is_empty() {
                    lines.push(Line::from("feedback:"));
                    for l in feedback.lines() {
                        lines.push(Line::from(l.to_string()));
                    }
                }

                self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
                    history_cell::new_user_approval_decision(lines),
                )));
            }
            ApprovalRequest::ApplyPatch { .. } => {
                // No history line for patch approval decisions.
            }
        }

        let op = match &self.approval_request {
            ApprovalRequest::Exec { id, .. } => Op::ExecApproval {
                id: id.clone(),
                decision,
            },
            ApprovalRequest::ApplyPatch { id, .. } => Op::PatchApproval {
                id: id.clone(),
                decision,
            },
        };

        self.app_event_tx.send(AppEvent::CodexOp(op));
        self.done = true;
    }

    /// Returns `true` once the user has made a decision and the widget no
    /// longer needs to be displayed.
    pub(crate) fn is_complete(&self) -> bool {
        self.done
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        let prompt_height = self.get_confirmation_prompt_height(width);
        let option_rows = self.select_options.len().max(1) as u16;
        prompt_height
            .saturating_add(1) // title line
            .saturating_add(option_rows)
    }
}

impl WidgetRef for &UserApprovalWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let prompt_height = self.get_confirmation_prompt_height(area.width);
        let [prompt_chunk, response_chunk] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(prompt_height), Constraint::Min(0)])
            .areas(area);

        let [title_area, options_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(response_chunk);
        let title = match &self.approval_request {
            ApprovalRequest::Exec { .. } => "Allow command?",
            ApprovalRequest::ApplyPatch { .. } => "Apply changes?",
        };
        let title_line = Line::from(vec!["▌ ".dim(), title.to_string().bold()]);
        Paragraph::new(title_line).render(title_area, buf);

        self.confirmation_prompt.clone().render(prompt_chunk, buf);

        let rows: Vec<GenericDisplayRow> = self
            .select_options
            .iter()
            .map(|opt| GenericDisplayRow {
                name: UserApprovalWidget::format_option_label(opt),
                match_indices: None,
                is_current: false,
                description: Some(opt.description.to_string()),
            })
            .collect();

        if options_area.height > 0 {
            render_rows(
                options_area,
                buf,
                &rows,
                &self.scroll_state,
                self.select_options.len(),
                false,
                "no options",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn lowercase_shortcut_is_accepted() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let req = ApprovalRequest::Exec {
            id: "1".to_string(),
            command: vec!["echo".to_string()],
            reason: None,
        };
        let mut widget = UserApprovalWidget::new(req, tx);
        widget.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(widget.is_complete());
        let mut events: Vec<AppEvent> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(events.iter().any(|e| matches!(
            e,
            AppEvent::CodexOp(Op::ExecApproval {
                decision: ReviewDecision::Approved,
                ..
            })
        )));
    }

    #[test]
    fn uppercase_shortcut_is_accepted() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let req = ApprovalRequest::Exec {
            id: "2".to_string(),
            command: vec!["echo".to_string()],
            reason: None,
        };
        let mut widget = UserApprovalWidget::new(req, tx);
        widget.handle_key_event(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE));
        assert!(widget.is_complete());
        let mut events: Vec<AppEvent> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(events.iter().any(|e| matches!(
            e,
            AppEvent::CodexOp(Op::ExecApproval {
                decision: ReviewDecision::Approved,
                ..
            })
        )));
    }
}
