use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::ChatWidget;
use crate::file_search::FileSearchManager;
use crate::get_git_diff::get_git_diff;
use crate::git_warning_screen::GitWarningOutcome;
use crate::git_warning_screen::GitWarningScreen;
use crate::login_screen::LoginScreen;
use crate::mouse_capture::MouseCapture;
use crate::scroll_event_helper::ScrollEventHelper;
use crate::slash_command::SlashCommand;
use crate::tui;
use codex_core::config::Config;
use codex_core::protocol::Event;
use codex_core::protocol::Op;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;

/// Template for compact summary continuation prompt
const COMPACT_SUMMARY_TEMPLATE: &str = concat!(
    "This chat is a continuation of a previous conversation. ",
    "After providing the summary, acknowledge that /compact command has been applied. ",
    "Here is the summary of the previous conversation:\n\n{}"
);

/// Creates the initial prompt for a compacted conversation
fn create_compact_summary_prompt(summary_text: &str) -> String {
    if summary_text.trim().is_empty() {
        "Previous conversation has been summarized.".to_string()
    } else {
        COMPACT_SUMMARY_TEMPLATE.replace("{}", summary_text.trim())
    }
}

/// Top-level application state: which full-screen view is currently active.
#[allow(clippy::large_enum_variant)]
enum AppState<'a> {
    /// The main chat UI is visible.
    Chat {
        /// Boxed to avoid a large enum variant and reduce the overall size of
        /// `AppState`.
        widget: Box<ChatWidget<'a>>,
    },
    /// The login screen for the OpenAI provider.
    Login { screen: LoginScreen },
    /// The start-up warning that recommends running codex inside a Git repo.
    GitWarning { screen: GitWarningScreen },
}

/// State for tracking a pending summarization request
struct PendingSummarization {
    /// Buffer to collect the summary response
    summary_buffer: String,
}

/// Aggregate parameters needed to create a `ChatWidget`, as creation may be
/// deferred until after the Git warning screen is dismissed.
#[derive(Clone)]
struct ChatWidgetArgs {
    config: Config,
    initial_prompt: Option<String>,
    initial_images: Vec<PathBuf>,
}

pub(crate) struct App<'a> {
    app_event_tx: AppEventSender,
    app_event_rx: Receiver<AppEvent>,
    app_state: AppState<'a>,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    config: Config,

    file_search: FileSearchManager,

    /// Stored parameters needed to instantiate the ChatWidget later, e.g.,
    /// after dismissing the Git-repo warning.
    chat_args: Option<ChatWidgetArgs>,

    /// Tracks pending summarization requests for the compact feature
    pending_summarization: Option<PendingSummarization>,
}

impl<'a> App<'a> {
    pub(crate) fn new(
        config: Config,
        initial_prompt: Option<String>,
        show_login_screen: bool,
        show_git_warning: bool,
        initial_images: Vec<std::path::PathBuf>,
    ) -> Self {
        let (app_event_tx, app_event_rx) = channel();
        let app_event_tx = AppEventSender::new(app_event_tx);
        let scroll_event_helper = ScrollEventHelper::new(app_event_tx.clone());

        // Spawn a dedicated thread for reading the crossterm event loop and
        // re-publishing the events as AppEvents, as appropriate.
        {
            let app_event_tx = app_event_tx.clone();
            std::thread::spawn(move || {
                while let Ok(event) = crossterm::event::read() {
                    match event {
                        crossterm::event::Event::Key(key_event) => {
                            app_event_tx.send(AppEvent::KeyEvent(key_event));
                        }
                        crossterm::event::Event::Resize(_, _) => {
                            app_event_tx.send(AppEvent::Redraw);
                        }
                        crossterm::event::Event::Mouse(MouseEvent {
                            kind: MouseEventKind::ScrollUp,
                            ..
                        }) => {
                            scroll_event_helper.scroll_up();
                        }
                        crossterm::event::Event::Mouse(MouseEvent {
                            kind: MouseEventKind::ScrollDown,
                            ..
                        }) => {
                            scroll_event_helper.scroll_down();
                        }
                        crossterm::event::Event::Paste(pasted) => {
                            use crossterm::event::KeyModifiers;

                            for ch in pasted.chars() {
                                let key_event = match ch {
                                    '\n' | '\r' => {
                                        // Represent newline as <Shift+Enter> so that the bottom
                                        // pane treats it as a literal newline instead of a submit
                                        // action (submission is only triggered on Enter *without*
                                        // any modifiers).
                                        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
                                    }
                                    _ => KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
                                };
                                app_event_tx.send(AppEvent::KeyEvent(key_event));
                            }
                        }
                        _ => {
                            // Ignore any other events.
                        }
                    }
                }
            });
        }

        let (app_state, chat_args) = if show_login_screen {
            (
                AppState::Login {
                    screen: LoginScreen::new(app_event_tx.clone(), config.codex_home.clone()),
                },
                Some(ChatWidgetArgs {
                    config: config.clone(),
                    initial_prompt,
                    initial_images,
                }),
            )
        } else if show_git_warning {
            (
                AppState::GitWarning {
                    screen: GitWarningScreen::new(),
                },
                Some(ChatWidgetArgs {
                    config: config.clone(),
                    initial_prompt,
                    initial_images,
                }),
            )
        } else {
            let chat_widget = ChatWidget::new(
                config.clone(),
                app_event_tx.clone(),
                initial_prompt,
                initial_images,
            );
            (
                AppState::Chat {
                    widget: Box::new(chat_widget),
                },
                None,
            )
        };

        let file_search = FileSearchManager::new(config.cwd.clone(), app_event_tx.clone());
        Self {
            app_event_tx,
            app_event_rx,
            app_state,
            config,
            file_search,
            chat_args,
            pending_summarization: None,
        }
    }

    /// Clone of the internal event sender so external tasks (e.g. log bridge)
    /// can inject `AppEvent`s.
    pub fn event_sender(&self) -> AppEventSender {
        self.app_event_tx.clone()
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut tui::Tui,
        mouse_capture: &mut MouseCapture,
    ) -> Result<()> {
        // Insert an event to trigger the first render.
        let app_event_tx = self.app_event_tx.clone();
        app_event_tx.send(AppEvent::Redraw);

        while let Ok(event) = self.app_event_rx.recv() {
            match event {
                AppEvent::Redraw => {
                    self.draw_next_frame(terminal)?;
                }
                AppEvent::KeyEvent(key_event) => {
                    match key_event {
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            ..
                        } => {
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    if widget.on_ctrl_c() {
                                        self.app_event_tx.send(AppEvent::ExitRequest);
                                    }
                                }
                                AppState::Login { .. } | AppState::GitWarning { .. } => {
                                    // No-op.
                                }
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('d'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            ..
                        } => {
                            self.app_event_tx.send(AppEvent::ExitRequest);
                        }
                        _ => {
                            self.dispatch_key_event(key_event);
                        }
                    };
                }
                AppEvent::Scroll(scroll_delta) => {
                    self.dispatch_scroll_event(scroll_delta);
                }
                AppEvent::CodexEvent(event) => {
                    // Check if we're waiting for a summarization response
                    if let Some(ref mut pending) = self.pending_summarization {
                        if let Event {
                            msg: codex_core::protocol::EventMsg::AgentMessage(ref msg),
                            ..
                        } = event
                        {
                            // Collect the summary response
                            pending.summary_buffer.push_str(&msg.message);
                            pending.summary_buffer.push('\n');
                        } else if let Event {
                            msg: codex_core::protocol::EventMsg::TaskComplete(_),
                            ..
                        } = event
                        {
                            // Task is complete, now create a new widget with the summary
                            if let Some(pending) = self.pending_summarization.take() {
                                let summary =
                                    create_compact_summary_prompt(&pending.summary_buffer);

                                // Create new widget with summary as initial prompt
                                let new_widget = Box::new(ChatWidget::new(
                                    self.config.clone(),
                                    self.app_event_tx.clone(),
                                    Some(summary),
                                    Vec::new(),
                                ));
                                self.app_state = AppState::Chat { widget: new_widget };
                                self.app_event_tx.send(AppEvent::Redraw);
                                continue;
                            }
                        }
                    }

                    self.dispatch_codex_event(event);
                }
                AppEvent::ExitRequest => {
                    break;
                }
                AppEvent::CodexOp(op) => match &mut self.app_state {
                    AppState::Chat { widget } => widget.submit_op(op),
                    AppState::Login { .. } | AppState::GitWarning { .. } => {}
                },
                AppEvent::LatestLog(line) => match &mut self.app_state {
                    AppState::Chat { widget } => widget.update_latest_log(line),
                    AppState::Login { .. } | AppState::GitWarning { .. } => {}
                },
                AppEvent::DispatchCommand(command) => match command {
                    SlashCommand::New => {
                        let new_widget = Box::new(ChatWidget::new(
                            self.config.clone(),
                            self.app_event_tx.clone(),
                            None,
                            Vec::new(),
                        ));
                        self.app_state = AppState::Chat { widget: new_widget };
                        self.app_event_tx.send(AppEvent::Redraw);
                    }
                    SlashCommand::ToggleMouseMode => {
                        if let Err(e) = mouse_capture.toggle() {
                            tracing::error!("Failed to toggle mouse mode: {e}");
                        }
                    }
                    SlashCommand::Quit => {
                        break;
                    }
                    SlashCommand::Diff => {
                        let (is_git_repo, diff_text) = match get_git_diff() {
                            Ok(v) => v,
                            Err(e) => {
                                let msg = format!("Failed to compute diff: {e}");
                                if let AppState::Chat { widget } = &mut self.app_state {
                                    widget.add_diff_output(msg);
                                }
                                continue;
                            }
                        };

                        if let AppState::Chat { widget } = &mut self.app_state {
                            let text = if is_git_repo {
                                diff_text
                            } else {
                                "`/diff` — _not inside a git repository_".to_string()
                            };
                            widget.add_diff_output(text);
                        }
                    }
                    SlashCommand::Compact => {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            // Submit the summarization request to the current widget
                            widget.submit_op(Op::SummarizeContext);

                            // Set up tracking for the summary response
                            self.pending_summarization = Some(PendingSummarization {
                                summary_buffer: String::new(),
                            });
                        }
                    }
                },
                AppEvent::StartFileSearch(query) => {
                    self.file_search.on_user_query(query);
                }
                AppEvent::FileSearchResult { query, matches } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_file_search_result(query, matches);
                    }
                }
            }
        }
        terminal.clear()?;

        Ok(())
    }

    fn draw_next_frame(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                terminal.draw(|frame| frame.render_widget_ref(&**widget, frame.area()))?;
            }
            AppState::Login { screen } => {
                terminal.draw(|frame| frame.render_widget_ref(&*screen, frame.area()))?;
            }
            AppState::GitWarning { screen } => {
                terminal.draw(|frame| frame.render_widget_ref(&*screen, frame.area()))?;
            }
        }
        Ok(())
    }

    /// Dispatch a KeyEvent to the current view and let it decide what to do
    /// with it.
    fn dispatch_key_event(&mut self, key_event: KeyEvent) {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                widget.handle_key_event(key_event);
            }
            AppState::Login { screen } => screen.handle_key_event(key_event),
            AppState::GitWarning { screen } => match screen.handle_key_event(key_event) {
                GitWarningOutcome::Continue => {
                    // User accepted – switch to chat view.
                    let args = match self.chat_args.take() {
                        Some(args) => args,
                        None => panic!("ChatWidgetArgs already consumed"),
                    };

                    let widget = Box::new(ChatWidget::new(
                        args.config,
                        self.app_event_tx.clone(),
                        args.initial_prompt,
                        args.initial_images,
                    ));
                    self.app_state = AppState::Chat { widget };
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                GitWarningOutcome::Quit => {
                    self.app_event_tx.send(AppEvent::ExitRequest);
                }
                GitWarningOutcome::None => {
                    // do nothing
                }
            },
        }
    }

    fn dispatch_scroll_event(&mut self, scroll_delta: i32) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_scroll_delta(scroll_delta),
            AppState::Login { .. } | AppState::GitWarning { .. } => {}
        }
    }

    fn dispatch_codex_event(&mut self, event: Event) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_codex_event(event),
            AppState::Login { .. } | AppState::GitWarning { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use codex_core::protocol::AgentMessageEvent;
    use codex_core::protocol::EventMsg;
    use codex_core::protocol::TaskCompleteEvent;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_create_compact_summary_prompt_with_content() {
        let summary_text = "User asked about Rust. I explained ownership and borrowing.";
        let result = create_compact_summary_prompt(summary_text);

        let expected = COMPACT_SUMMARY_TEMPLATE.replace(
            "{}",
            "User asked about Rust. I explained ownership and borrowing.",
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_create_compact_summary_prompt_empty_content() {
        let result = create_compact_summary_prompt("");
        assert_eq!(result, "Previous conversation has been summarized.");

        let result_whitespace = create_compact_summary_prompt("   \n\t   ");
        assert_eq!(
            result_whitespace,
            "Previous conversation has been summarized."
        );
    }

    #[test]
    fn test_pending_summarization_state_management() {
        let mut pending = PendingSummarization {
            summary_buffer: String::new(),
        };

        // Simulate collecting summary pieces
        pending.summary_buffer.push_str("First part of summary");
        pending.summary_buffer.push('\n');
        pending.summary_buffer.push_str("Second part of summary");

        let expected = "First part of summary\nSecond part of summary";
        assert_eq!(pending.summary_buffer, expected);

        // Test that create_compact_summary_prompt works with collected buffer
        let prompt = create_compact_summary_prompt(&pending.summary_buffer);
        assert!(prompt.contains("First part of summary"));
        assert!(prompt.contains("Second part of summary"));
    }

    #[test]
    fn test_compact_summary_template_integrity() {
        // Ensure the template has expected structure and placeholder
        assert!(COMPACT_SUMMARY_TEMPLATE.contains("{}"));
        assert!(COMPACT_SUMMARY_TEMPLATE.contains("continuation of a previous conversation"));
        assert!(COMPACT_SUMMARY_TEMPLATE.contains("/compact command has been applied"));
    }

    #[test]
    fn test_agent_message_event_creation() {
        // Test that we can create the events we expect to handle
        let msg_event = EventMsg::AgentMessage(AgentMessageEvent {
            message: "This is a test summary".to_string(),
        });

        if let EventMsg::AgentMessage(agent_msg) = msg_event {
            assert_eq!(agent_msg.message, "This is a test summary");
        } else {
            panic!("Expected AgentMessage event");
        }

        let task_complete_event = EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: Some("Final message".to_string()),
        });

        matches!(task_complete_event, EventMsg::TaskComplete(_));
    }

    #[test]
    fn test_multiline_summary_handling() {
        let multiline_summary = "Line 1: User question\nLine 2: My response\nLine 3: Follow-up";
        let result = create_compact_summary_prompt(multiline_summary);

        assert!(result.contains("Line 1: User question"));
        assert!(result.contains("Line 2: My response"));
        assert!(result.contains("Line 3: Follow-up"));
        assert!(result.contains("continuation of a previous conversation"));
    }

    #[test]
    fn test_summary_buffer_accumulation() {
        let mut buffer = String::new();

        // Simulate the way we accumulate messages in pending_summarization
        buffer.push_str("First message part");
        buffer.push('\n');
        buffer.push_str("Second message part");
        buffer.push('\n');
        buffer.push_str("Final message part");

        let prompt = create_compact_summary_prompt(&buffer);

        // Should contain all parts
        assert!(prompt.contains("First message part"));
        assert!(prompt.contains("Second message part"));
        assert!(prompt.contains("Final message part"));

        // Should preserve newlines in the content
        let trimmed_buffer = buffer.trim();
        assert!(prompt.contains(trimmed_buffer));
    }
}
