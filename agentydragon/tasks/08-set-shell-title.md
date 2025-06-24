# Task 08: Set Shell Title to Reflect Session Status

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

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

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Use ANSI escape code `\033]0;<title>\007` to set the terminal title.
- Extend the session JSON schema to include a `title` field.
- Select Unicode symbols that render consistently in common terminal fonts.