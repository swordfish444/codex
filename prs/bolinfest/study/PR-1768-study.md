**DOs**
- Gate on key press events: Process one-shot actions only on `KeyEventKind::Press` to avoid duplicate or unintended triggers, and apply this consistently across handlers.
  ```rust
  use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

  fn handle_key_event(&mut self, ev: KeyEvent) {
      if ev.kind != KeyEventKind::Press {
          return;
      }
      match ev {
          KeyEvent { code: KeyCode::Char('c'), modifiers, .. }
              if modifiers.contains(KeyModifiers::CONTROL) => self.quit(),
          _ => {}
      }
  }
  ```

- Use `LazyLock` option sets: Define context-specific approval options (command vs patch) once, with styled labels and direct key bindings.
  ```rust
  use std::sync::LazyLock;
  use crossterm::event::KeyCode;
  use ratatui::text::Line;
  use codex_core::protocol::ReviewDecision;

  struct SelectOption {
      label: Line<'static>,
      description: &'static str,
      key: KeyCode,
      decision: ReviewDecision,
  }

  static COMMAND_SELECT_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| vec![
      SelectOption {
          label: Line::from(vec!["Y".underlined(), "es".into()]),
          description: "Approve and run the command",
          key: KeyCode::Char('y'),
          decision: ReviewDecision::Approved,
      },
      SelectOption {
          label: Line::from(vec!["A".underlined(), "lways".into()]),
          description: "Approve for this session",
          key: KeyCode::Char('a'),
          decision: ReviewDecision::ApprovedForSession,
      },
      SelectOption {
          label: Line::from(vec!["N".underlined(), "o".into()]),
          description: "Do not run the command",
          key: KeyCode::Char('n'),
          decision: ReviewDecision::Denied,
      },
  ]);
  ```

- Prefer Stylize helpers for concise UI: Build prompts and labels using `Stylize` and inline `format!` variables.
  ```rust
  use ratatui::text::Line;
  use ratatui::prelude::Stylize;

  let cwd_str = cwd.display().to_string();
  let cmd = shell_words::join(&command);
  let prompt = vec![
      Line::from(vec!["codex".bold().magenta(), " wants to run:".into()]),
      Line::from(vec![cwd_str.dim(), "$".into(), format!(" {cmd}").into()]),
  ];
  ```

- Verify contrast on light and dark terminals: Choose styles that remain legible across themes; consider `reversed` or high-contrast fg/bg pairs.
  ```rust
  use ratatui::style::{Color, Style};
  use ratatui::prelude::Stylize;

  let selected = " Yes ".into().style(Style::new().bg(Color::Cyan).fg(Color::Black));
  let unselected = " No ".into().reversed(); // safe contrast across themes
  ```

- Support direct shortcuts and selection: Let users press `y/a/n` (and `Esc`) in addition to arrow/enter navigation.
  ```rust
  use crossterm::event::KeyCode;

  if ev.kind == KeyEventKind::Press {
      if let Some(opt) = self.select_options.iter().find(|o| o.key == ev.code) {
          self.send_decision(opt.decision);
      }
  }
  ```

**DON'Ts**
- Handle non-press events for single-shot actions: Avoid mutating state on `Repeat`/`Release` or on every event without gating.
  ```rust
  // Anti-pattern: fires on press, repeat, and release
  fn on_key(&mut self, ev: KeyEvent) {
      self.bottom_pane.clear_ctrl_c_quit_hint(); // runs too often
  }
  ```

- Keep stale cross-implementation comments: Remove comments that tie Rust code to TS ordering; make Rust the source of truth.
  ```rust
  // Anti-pattern:
  // keep in same order as in the TS implementation
  // Preferred: model the options here without external coupling.
  ```

- Assume colors work everywhere: Avoid low-contrast combinations that disappear on light backgrounds (e.g., `DarkGray` on default fg).
  ```rust
  // Anti-pattern: may be unreadable on light themes
  let unselected = " No ".into().style(Style::new().bg(Color::DarkGray));
  ```

- Clear UI hints on every key transition: Only clear “press any key” or similar hints on `Press`, not on `Repeat`/`Release`.
  ```rust
  // Preferred
  if key_event.kind == KeyEventKind::Press {
      self.bottom_pane.clear_ctrl_c_quit_hint();
  }
  ```