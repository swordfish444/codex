+++
id = "08"
title = "Set Shell Title to Reflect Session Status"
status = "Done"
dependencies = "02,07,09,11,14,29"
last_updated = "2025-06-30T12:00:00.000000"
+++

# Task 08: Set Shell Title to Reflect Session Status

> *This task is specific to codex-rs.*

## Status

**General Status**: Done  
**Summary**: Implemented session title persistence, `/set-title` slash command, and real-time ANSI updates in both TUI and exec clients.

## Goal

Allow the CLI to update the terminal title bar to reflect the current session status—executing, thinking (sampling), idle, or waiting for approval decision—and persist the title with the session. Users should also be able to explicitly set a custom title.

## Acceptance Criteria

- Implement a slash command or API (`/set-title <title>`) for users to explicitly set the session title.
- Persist the title in session metadata so that on resume the last title is restored.
- Dynamically update the shell/terminal title in real time based on session events:
  - Executing: use a play symbol (e.g. ▶)
  - Thinking/sampling: use an hourglass or brain symbol (e.g. ⏳)
  - Idle: use a green dot or sleep symbol (e.g. 🟢)
  - Waiting for approval decision: use an attention-grabbing symbol (e.g. ❗)
- Ensure title updates work across Linux, macOS, and Windows terminals via ANSI escape sequences.

## Implementation
**Note**: Populate this section with a concise high-level plan before beginning detailed implementation.

**Planned approach**  
- Extend the session protocol schema (`SessionConfiguredEvent`) in `codex-rs/core` to include an optional `title` field and introduce a new `SessionUpdatedTitleEvent` type.  
- Add a `SetTitle { title: String }` variant to the `Op` enum for custom titles and implement the `/set-title <text>` slash command in the TUI crates (`tui/src/slash_command.rs`, `tui/src/app_event.rs`, and `tui/src/app.rs`).  
- Modify the core agent loop to handle `Op::SetTitle`: persist the new title in session metadata, emit a `SessionUpdatedTitleEvent`, and include the persisted title in `SessionConfiguredEvent` on startup/resume.  
- Implement event listeners in both the interactive TUI (`tui/src/chatwidget.rs`) and non-interactive exec client (`exec/src/event_processor.rs`) that respond to session, title, and lifecycle events (session start, task begin/end, reasoning, idle, approval) by emitting ANSI escape sequences (`\x1b]0;<symbol> <title>\x07`) to update the terminal title bar.  
- Choose consistent Unicode symbols for each session state—executing (▶), thinking (⏳), idle (🟢), awaiting approval (❗)—and apply these as status indicators prefixed to the title.  
- On session startup or resume, restore the last persisted title or fall back to a default if none exists.

**How it works**  
- Users type `/set-title MyTitle` to set a custom session title; the core persists it and broadcasts a `SessionUpdatedTitleEvent`.  
- Clients print the appropriate ANSI escape code to update the terminal title before rendering UI or logs, reflecting real-time session state via the selected status symbol prefix.

## Notes

- Use ANSI escape code `\033]0;<title>\007` to set the terminal title.
- Extend the session JSON schema to include a `title` field.
- Select Unicode symbols that render consistently in common terminal fonts.
