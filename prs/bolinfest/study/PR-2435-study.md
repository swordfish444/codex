**DOs**
- **Sync Docs With Enums**: When changing `ReasoningEffort` (e.g., removing `None`), update `config.md` to match supported values.
  ```rust
  // protocol/src/config_types.rs
  #[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
  #[serde(rename_all = "lowercase")]
  pub enum ReasoningEffort {
      Minimal,
      Low,
      #[default]
      Medium,
      High,
  }
  ```
  ```md
  <!-- config.md -->
  Supported: "minimal", "low", "medium" (default), "high".
  Note: to minimize reasoning, choose "minimal".
  ```

- **Make Esc Cancel**: Bind `Esc` to cancel/close popups; reserve `Enter` for accept.
  ```rust
  match key_event {
      KeyEvent { code: KeyCode::Esc, .. } => self.cancel(),
      KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => self.accept(),
      KeyEvent { code: KeyCode::Up, .. } => self.move_up(),
      KeyEvent { code: KeyCode::Down, .. } => self.move_down(),
      _ => {}
  }
  ```

- **Add Clear Titles To Popups**: Provide a title (and optional subtitle/footer hint) for selection views.
  ```rust
  // bottom_pane/mod.rs
  bottom_pane.show_selection_view(
      "Select model and reasoning level".to_string(),
      Some("Affects this and future Codex CLI session".to_string()),
      Some("Enter: confirm • Esc: cancel".to_string()),
      items,
  );
  ```
  ```rust
  // In the view's render()
  let title_para = Paragraph::new(Line::from(vec![
      Span::styled("▌ ", Style::default().add_modifier(Modifier::DIM)),
      Span::styled(self.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
  ]));
  title_para.render(title_area, buf);
  ```

**DON’Ts**
- **Don’t Document Removed Values**: Avoid referencing `"none"` in docs once the enum no longer supports it.
  ```md
  <!-- Bad -->
  model_reasoning_effort = "none"  # disable reasoning

  <!-- Good -->
  # Use "minimal" to minimize reasoning.
  ```

- **Don’t Bind Esc To Accept**: Esc should never trigger selection actions.
  ```rust
  // Bad
  KeyEvent { code: KeyCode::Esc, .. } => self.accept(),

  // Good
  KeyEvent { code: KeyCode::Esc, .. } => self.cancel(),
  ```

- **Don’t Ship Untitled Popups**: Avoid selection popups without a visible, descriptive title.
  ```rust
  // Bad: missing title/subtitle/footer
  ListSelectionView::new(items, app_event_tx);

  // Good: clear, guided UI
  ListSelectionView::new(
      "Choose preset".to_string(),
      Some("Applies model and reasoning effort".to_string()),
      Some("Enter to confirm, Esc to cancel".to_string()),
      items,
      app_event_tx,
  );
  ```