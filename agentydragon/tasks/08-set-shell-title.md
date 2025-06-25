# Task 08: Set Shell Title to Reflect Session Status

> *This task is specific to codex-rs.*

## Status

**General Status**: Complete  
**Summary**: All acceptance criteria have been implemented and verified. Shell title functionality is fully operational.

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
**Note**: Final implementation applied; see detailed design and behavior below.

**How it was implemented**  
- Extended the session protocol schema (`SessionConfiguredEvent`) to include an optional `title` field, enabling persistence of the shell title across sessions.  
- Added a new slash command `/set-title <text>` in the TUI (`slash_command.rs` and `app.rs`) that emits a dedicated `Op::SetTitle` operation carrying the user-provided title.  
- Updated the core agent loop (`codex-core`) to store the latest title in session metadata and emit a `SessionUpdatedTitleEvent` (alongside `SessionConfiguredEvent`) when the title changes.  
- In both the interactive TUI (`tui/src/chatwidget.rs`) and non-interactive exec client (`exec/src/event_processor.rs`), hooked into session events (startup, title updates, task begin/complete, thinking/idle states, approval prompts) to send ANSI escape sequences (`\x1b]0;<title>\x07`) to the terminal before rendering, ensuring real-time title updates.  
- Selected consistent Unicode status symbols (▶ for executing, ⏳ for thinking, 🟢 for idle, ❗ for awaiting approval) and prepended them to the title text.  
- On startup (SessionConfiguredEvent), restored the last persisted title if present, falling back to a configurable default (e.g. “Codex CLI”).

**How it works**  
- **Slash command**: when the user types `/set-title My Title`, the composer dispatches `Op::SetTitle("My Title")` instead of a regular user-input message.  
- **Core storage**: the core session handler persists the new title in memory and in the session JSON file under the `title` key.  
- **Event broadcast**: the core emits a `SessionUpdatedTitleEvent` (or extends `SessionConfiguredEvent` on resume) carrying the new title.  
- **ANSI update**: the TUI and exec clients listen for title-related events and immediately print the ANSI escape sequence (`\x1b]0;{symbol} {title}\x07`) to stdout before drawing UI or logs. Terminals on Linux, macOS, and Windows (supported via ANSI) update their window/tab title accordingly.  
- **Dynamic status**: on key lifecycle events (task start → ▶, reasoning → ⏳ animation, task complete → 🟢, approval overlays → ❗), clients format and emit the corresponding status symbol and the active title to visually reflect the current session state in the shell title.

## Notes

- Use ANSI escape code `\033]0;<title>\007` to set the terminal title.
- Extend the session JSON schema to include a `title` field.
- Select Unicode symbols that render consistently in common terminal fonts.