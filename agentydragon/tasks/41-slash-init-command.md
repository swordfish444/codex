+++
id = "41"
title = "Slash-command /init to load init prompt into composer"
status = "Not started"
freeform_status = ""
dependencies = ""
last_updated = "2025-06-25T11:23:30Z"
+++

# Task 41: Slash-command /init to load init prompt into composer

> *This task is specific to codex-rs.*

## Acceptance Criteria

- Typing `/init` in the chat composer should load the contents of `codex-rs/code/init.md` into the input buffer.
- `/init` appears in the slash-command menu alongside other commands.
- After executing `/init`, the composer shows the init prompt, ready for editing.

## Implementation

- Add a new slash-command identifier `/init` in the command dispatch logic (e.g. in `ChatComposer` or equivalent).
- On `/init`, read `codex-rs/code/init.md` (relative to the repository root) and inject its text into the composer buffer.
- Ensure the slash-menu and feedback UI treat `/init` consistently with other commands.
- Write unit tests to verify that `/init` populates the composer correctly without losing focus.

## Notes

Link to the init prompt source: `codex-rs/code/init.md`.
