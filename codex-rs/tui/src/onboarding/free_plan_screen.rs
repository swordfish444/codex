use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepState;
use crate::onboarding::onboarding_screen::StepStateProvider;

/// Informational screen shown when the user is authenticated with a free
/// ChatGPT account. Provides a link to pricing and exits on Enter.
pub(crate) struct FreePlanWidget {
    pub event_tx: AppEventSender,
}

impl StepStateProvider for FreePlanWidget {
    fn get_step_state(&self) -> StepState {
        // This screen is interactive (waits for Enter), so it is always in progress.
        StepState::InProgress
    }
}

impl KeyboardHandler for FreePlanWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if matches!(key_event.code, KeyCode::Enter) {
            self.event_tx.send(AppEvent::ExitRequest);
        }
    }
}

impl WidgetRef for &FreePlanWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<Line> = vec![
            Line::from(""),
            Line::from("  You’re currently signed in using a free ChatGPT account."),
            Line::from(
                "  To use Codex with your ChatGPT plan, upgrade to a Pro, Plus, and Team account.",
            ),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                // Use an OSC 8 hyperlink so terminals render a clickable URL
                Span::styled(
                    "\u{1b}]8;;https://openai.com/chatgpt/pricing\u{7}https://openai.com/chatgpt/pricing\u{1b}]8;;\u{7}",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
            ]),
            Line::from(""),
            Line::from("  Press Enter to exit").style(Style::default().add_modifier(Modifier::DIM)),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
