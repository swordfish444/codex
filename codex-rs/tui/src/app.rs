use crate::app_event::AppEvent;
use crate::confirm_ctrl_d::ConfirmCtrlD;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::ChatWidget;
use crate::git_warning_screen::GitWarningOutcome;
use crate::git_warning_screen::GitWarningScreen;
use crate::login_screen::LoginScreen;
use crate::mouse_capture::MouseCapture;
use crate::scroll_event_helper::ScrollEventHelper;
use crate::slash_command::SlashCommand;
use crate::tui;
use codex_core::config::{Config, ConfigOverrides};
use codex_core::protocol::{Event, EventMsg, Op, SessionConfiguredEvent};
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;
use std::time::Instant;

use codex_core::ResponseItem;
use uuid::Uuid;

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

pub(crate) struct App<'a> {
    app_event_tx: AppEventSender,
    app_event_rx: Receiver<AppEvent>,
    app_state: AppState<'a>,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    config: Config,

    /// Stored parameters needed to instantiate the ChatWidget later, e.g.,
    /// after dismissing the Git-repo warning.
    chat_args: Option<ChatWidgetArgs>,
    session_id: Option<Uuid>,
    /// Tracks Ctrl+D confirmation state when enabled in config.
    confirm_ctrl_d: ConfirmCtrlD,
}

/// Aggregate parameters needed to create a `ChatWidget`, as creation may be
/// deferred until after the Git warning screen is dismissed.
#[derive(Clone)]
struct ChatWidgetArgs {
    config: Config,
    initial_prompt: Option<String>,
    initial_images: Vec<PathBuf>,
}

/// Parse raw argument string for `/mount-add host=... container=... mode=...`.
fn parse_mount_add_args(raw: &str) -> Result<(std::path::PathBuf, std::path::PathBuf, String), String> {
    let mut host = None;
    let mut container = None;
    let mut mode = "rw".to_string();
    for token in raw.split_whitespace() {
        let mut parts = token.splitn(2, '=');
        let key = parts.next().unwrap();
        let value = parts.next().ok_or_else(|| format!("invalid argument '{}'", token))?;
        match key {
            "host" => host = Some(std::path::PathBuf::from(value)),
            "container" => container = Some(std::path::PathBuf::from(value)),
            "mode" => mode = value.to_string(),
            _ => return Err(format!("unknown argument '{}'", key)),
        }
    }
    let host = host.ok_or_else(|| "missing 'host' argument".to_string())?;
    let container = container.ok_or_else(|| "missing 'container' argument".to_string())?;
    Ok((host, container, mode))
}

/// Parse raw argument string for `/mount-remove container=...`.
fn parse_mount_remove_args(raw: &str) -> Result<std::path::PathBuf, String> {
    let mut container = None;
    for token in raw.split_whitespace() {
        let mut parts = token.splitn(2, '=');
        let key = parts.next().unwrap();
        let value = parts.next().ok_or_else(|| format!("invalid argument '{}'", token))?;
        if key == "container" {
            container = Some(std::path::PathBuf::from(value));
        } else {
            return Err(format!("unknown argument '{}'", key));
        }
    }
    container.ok_or_else(|| "missing 'container' argument".to_string())
}

/// Handle inline mount-add DSL event.
fn handle_inline_mount_add(config: &mut Config, raw: &str) -> Result<(), String> {
    let (host, container, mode) = parse_mount_add_args(raw)?;
    do_mount_add(config, &host, &container, &mode).map_err(|e| e.to_string())
}

/// Handle inline mount-remove DSL event.
fn handle_inline_mount_remove(config: &mut Config, raw: &str) -> Result<(), String> {
    let container = parse_mount_remove_args(raw)?;
    do_mount_remove(config, &container).map_err(|e| e.to_string())
}

/// Perform mount-add: create symlink under cwd and update sandbox policy.
fn do_mount_add(
    config: &mut Config,
    host: &std::path::PathBuf,
    container: &std::path::PathBuf,
    mode: &str,
) -> std::io::Result<()> {
    let host_abs = std::fs::canonicalize(host)?;
    let target = config.cwd.join(container);
    if target.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("target '{}' already exists", target.display()),
        ));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&host_abs, &target)?;
    #[cfg(windows)]
    {
        if host_abs.is_file() {
            std::os::windows::fs::symlink_file(&host_abs, &target)?;
        } else {
            std::os::windows::fs::symlink_dir(&host_abs, &target)?;
        }
    }
    if mode.contains('w') {
        config.sandbox_policy.allow_disk_write_folder(host_abs);
    }
    Ok(())
}

/// Perform mount-remove: remove symlink under cwd and revoke sandbox policy.
fn do_mount_remove(config: &mut Config, container: &std::path::PathBuf) -> std::io::Result<()> {
    let target = config.cwd.join(container);
    let host = std::fs::read_link(&target)?;
    std::fs::remove_file(&target)?;
    config.sandbox_policy.revoke_disk_write_folder(host);
    Ok(())
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

        Self {
            app_event_tx,
            app_event_rx,
            app_state,
            config: config.clone(),
            chat_args,
            session_id: None,
            confirm_ctrl_d: ConfirmCtrlD::new(
                config.tui.require_double_ctrl_d,
                config.tui.double_ctrl_d_timeout_secs,
            ),
        }
    }

    /// Clone of the internal event sender so external tasks (e.g. log bridge)
    /// can inject `AppEvent`s.
    pub fn event_sender(&self) -> AppEventSender {
        self.app_event_tx.clone()
    }

    /// Override the session ID for this UI instance (useful for session-resume).
    pub fn set_session_id(&mut self, id: Uuid) {
        self.session_id = Some(id);
    }

    /// Replay a previous session transcript into the chat widget.
    pub fn replay_items(&mut self, items: Vec<ResponseItem>) {
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.replay_items(items);
        }
    }

    /// Override the session ID for this UI instance (useful for session-resume).

    /// Returns the session ID assigned by the backend for this session, if available.
    pub fn session_id(&self) -> Option<Uuid> {
        self.session_id
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
            // Expire pending Ctrl+D confirmation and clear any prompt overlay.
            let now = Instant::now();
            self.confirm_ctrl_d.expire(now);
            if self.config.tui.require_double_ctrl_d && !self.confirm_ctrl_d.is_confirming() {
                if let AppState::Chat { widget } = &mut self.app_state {
                    widget.clear_exit_confirmation_prompt();
                }
            }
            match event {
                AppEvent::Redraw => {
                    self.draw_next_frame(terminal)?;
                }
                AppEvent::InlineMountAdd(args) => {
                    if let Err(err) = handle_inline_mount_add(&mut self.config, &args) {
                        tracing::error!("mount-add failed: {err}");
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::InlineMountRemove(args) => {
                    if let Err(err) = handle_inline_mount_remove(&mut self.config, &args) {
                        tracing::error!("mount-remove failed: {err}");
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::MountAdd { host, container, mode } => {
                    if let Err(err) = do_mount_add(&mut self.config, &host, &container, &mode) {
                        tracing::error!("mount-add failed: {err}");
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::MountRemove { container } => {
                    if let Err(err) = do_mount_remove(&mut self.config, &container) {
                        tracing::error!("mount-remove failed: {err}");
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::ConfigReloadRequest(diff) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.push_config_reload(diff);
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::ConfigReloadApply => {
                    match Config::load_with_cli_overrides(Vec::new(), ConfigOverrides::default()) {
                        Ok(new_cfg) => {
                            self.config = new_cfg.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.update_config(new_cfg);
                            }
                        }
                        Err(e) => tracing::error!("Failed to reload config.toml: {e}"),
                    }
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::ConfigReloadIgnore => {
                    self.app_event_tx.send(AppEvent::Redraw);
                }
                AppEvent::KeyEvent(key_event) => {
                    match key_event {
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            ..
                        } => {
                            // Forward interrupt to ChatWidget when active.
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    widget.submit_op(Op::Interrupt);
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
                        // Handle Ctrl+D exit confirmation when enabled.
                        let now = Instant::now();
                        if self.confirm_ctrl_d.handle(now) {
                            break;
                        }
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.show_exit_confirmation_prompt(
                                "Press Ctrl+D again to confirm exit".to_string(),
                            );
                        }
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
                    SlashCommand::EditPrompt => {
                        // External-editor prompt handled inline by the composer; no-op here.
                    }
                    SlashCommand::Quit => {
                        break;
                    }
                    SlashCommand::MountAdd => {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.push_mount_add_interactive();
                            self.app_event_tx.send(AppEvent::Redraw);
                        }
                    }
                    SlashCommand::MountRemove => {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.push_mount_remove_interactive();
                            self.app_event_tx.send(AppEvent::Redraw);
                        }
                    }
                },
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
        // Capture session ID when the session is initially configured
        if let EventMsg::SessionConfigured(SessionConfiguredEvent { session_id, .. }) = &event.msg {
            self.session_id = Some(*session_id);
        }
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_codex_event(event),
            AppState::Login { .. } | AppState::GitWarning { .. } => {}
        }
    }
}
