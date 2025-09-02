**DOs**
- Bold, declarative names: prefer intent over specific commands.
  - Use enum variants that describe behavior, not binaries.
  - Example:
    ```rust
    // Before
    enum ParsedCommand {
        Ls { cmd: Vec<String>, path: Option<String> },
    }

    // After
    enum ParsedCommand {
        ListFiles { cmd: Vec<String>, path: Option<String> },
    }
    ```
- Thread parsed metadata end-to-end.
  - Capture `parsed_cmd` at emit time and pass it through UI layers.
  - Example:
    ```rust
    // protocol.rs
    #[derive(Serialize, Deserialize, Clone)]
    pub struct ExecCommandBeginEvent {
        pub call_id: String,
        pub command: Vec<String>,
        pub cwd: PathBuf,
        pub parsed_cmd: Vec<ParsedCommand>,
    }

    // chatwidget.rs (begin)
    EventMsg::ExecCommandBegin(ExecCommandBeginEvent { call_id, command, cwd, parsed_cmd }) => {
        self.running_commands.insert(
            call_id.clone(),
            RunningCommand { command: command.clone(), cwd: cwd.clone(), parsed_cmd: parsed_cmd.clone() },
        );
        self.active_history_cell = Some(HistoryCell::new_active_exec_command(command, parsed_cmd));
    }

    // chatwidget.rs (end)
    EventMsg::ExecCommandEnd { call_id, exit_code, stdout, stderr } => {
        let (command, parsed) = match self.running_commands.remove(&call_id) {
            Some(rc) => (rc.command, rc.parsed_cmd),
            None => (vec![format!("{call_id}")], vec![]),
        };
        self.add_to_history(HistoryCell::new_completed_exec_command(
            command,
            parsed,
            CommandOutput { exit_code, stdout, stderr },
        ));
    }
    ```
- Render commands with proper quoting.
  - Use `shlex::try_join` to reconstruct safe, user-facing strings; fall back gracefully.
  - Example:
    ```rust
    fn shlex_join_safe(command: &[String]) -> String {
        shlex::try_join(command.iter().map(|s| s.as_str())).unwrap_or_else(|_| command.join(" "))
    }

    // history_cell.rs
    let label = format!("🧹 {}", shlex_join_safe(cmd));
    ```
- Prefer concise, semantic UI summaries.
  - Show a compact list of parsed actions; reserve full command echo for unknowns.
  - Example:
    ```rust
    match parsed {
        ParsedCommand::Read { name, .. } => format!("📖 {name}"),
        ParsedCommand::ListFiles { cmd, path } => match path {
            Some(p) => format!("📂 {p}"),
            None => format!("📂 {}", shlex_join_safe(cmd)),
        },
        ParsedCommand::Search { query, path, cmd } => match (query, path) {
            (Some(q), Some(p)) => format!("🔎 {q} in {p}"),
            (Some(q), None) => format!("🔎 {q}"),
            (None, Some(p)) => format!("🔎 {p}"),
            (None, None) => format!("🔎 {}", shlex_join_safe(cmd)),
        },
        ParsedCommand::Format { .. } => "✨ Formatting".to_string(),
        ParsedCommand::Test { cmd } => format!("🧪 {}", shlex_join_safe(cmd)),
        ParsedCommand::Lint { cmd, .. } => format!("🧹 {}", shlex_join_safe(cmd)),
        ParsedCommand::Unknown { cmd } => format!("⌨️ {}", shlex_join_safe(cmd)),
    }
    ```
- Degrade gracefully when state is missing.
  - Keep robust fallbacks so UI doesn’t break if `RunningCommand` is absent.
  - Example:
    ```rust
    let (command, parsed) = match self.running_commands.remove(&call_id) {
        Some(rc) => (rc.command, rc.parsed_cmd),
        None => (vec![format!("{call_id}")], vec![]),
    };
    ```
- Show output selectively to reduce noise.
  - Only render stdout/stderr for failures in generic exec summaries; always include for error-specific flows.
  - Example:
    ```rust
    // history_cell.rs
    // only_err=true for generic exec; include_angle_pipe=false when not streaming
    lines.extend(output_lines(Some(&output), true, false));

    fn output_lines(
        output: Option<&CommandOutput>,
        only_err: bool,
        include_angle_pipe: bool,
    ) -> Vec<Line<'static>> { /* ... */ }
    ```
- Standardize UI glyphs and styling.
  - Use consistent symbols (e.g., "✔") and color conventions.
  - Example:
    ```rust
    // user_approval_widget.rs
    lines.push(Line::from(vec![
        "✔ ".fg(Color::Green),
        "You ".into(),
        "approved".bold(),
        " codex to run ".into(),
        shlex_join_safe(&command).bold(),
    ]));
    ```
- Guard new keybindings and UI changes against conflicts.
  - Audit existing bindings (e.g., PR #1773) and gate behavior with context where needed.
  - Example:
    ```rust
    if self.focus_is_chat() && key.modifiers.contains(KeyModifiers::CONTROL) {
        if key.code == KeyCode::Char('z') {
            widget.on_ctrl_z(); // ensure no conflict with existing handlers
        }
    }
    ```

**DON'Ts**
- Don’t name variants after specific binaries.
  - Avoid `Ls`; use `ListFiles`. Avoid platform assumptions (e.g., `dir` on Windows).
  - Bad:
    ```rust
    enum ParsedCommand { Ls { /* ... */ } }
    ```
  - Good:
    ```rust
    enum ParsedCommand { ListFiles { /* ... */ } }
    ```
- Don’t stringify commands with naive `join(" ")`.
  - This loses quoting and produces misleading output.
  - Bad:
    ```rust
    format!("🧪 {}", cmd.join(" "));
    ```
  - Good:
    ```rust
    format!("🧪 {}", shlex_join_safe(cmd));
    ```
- Don’t drop fallbacks when optional data is missing.
  - Handle `None` cases for running command lookups to avoid empty or broken UI states.
  - Bad:
    ```rust
    let rc = self.running_commands.remove(&call_id).unwrap(); // panic
    ```
  - Good:
    ```rust
    let (command, parsed) = match self.running_commands.remove(&call_id) {
        Some(rc) => (rc.command, rc.parsed_cmd),
        None => (vec![format!("{call_id}")], vec![]),
    };
    ```
- Don’t ignore parsed summaries in the executor UI.
  - Wire `parsed_cmd` through; don’t discard with `_` unless intentionally disabled.
  - Bad:
    ```rust
    parsed_cmd: _,
    ```
  - Good:
    ```rust
    parsed_cmd,
    ```
- Don’t show full stdout on success by default.
  - Keep summaries terse; surface logs only when failing or explicitly requested.
  - Bad:
    ```rust
    lines.extend(output_lines(Some(&output), false, true)); // always shows
    ```
  - Good:
    ```rust
    lines.extend(output_lines(Some(&output), true, false)); // errors only
    ```
- Don’t misrepresent search queries.
  - Do not “shorten” grep queries that contain slashes; only shorten paths.
  - Bad:
    ```rust
    let query = short_display_path(&pattern); // wrong for queries
    ```
  - Good:
    ```rust
    let query = pattern.clone(); // preserve slashes and regexes
    ```