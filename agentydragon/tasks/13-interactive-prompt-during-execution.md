+++
id = "13"
title = "Interactive Prompting and Commands While Executing"
status = "Merged"
dependencies = ""
last_updated = "2025-06-25T01:40:09.509881"
+++

# Task 13: Interactive Prompting and Commands While Executing

> *This task is specific to codex-rs.*

## Status

**General Status**: Merged  
**Summary**: Implemented interactive prompt overlay allowing user input during streaming without aborting runs.

## Goal

Allow users to interleave composing prompts and issuing slash-commands while the agent is actively executing (e.g. streaming completions), without aborting the current run.

## Acceptance Criteria

- While the LLM is streaming a response or executing a tool, the input box remains active for user edits and slash-commands.
- Sending a message or `/`-command does not implicitly cancel or abort the ongoing execution.
- Any tool invocation messages from the agent must still be immediately followed by their corresponding tool output messages (or the API will error).
- Ensure the TUI correctly preserves the stream and appends new user input at the bottom, scrolling as needed.
- No deadlocks or lost events if the agent finishes while the user is typing; buffer and render properly.
- Update tests to simulate concurrent user input during streaming and validate UI state.

## Implementation

**How it was implemented**  
- Modified `BottomPane::handle_key_event` in `tui/src/bottom_pane/mod.rs` to special-case the `StatusIndicatorView` while `is_task_running`, forwarding key events to `ChatComposer` and preserving the overlay.
- Updated `BottomPane::render_ref` to always render the composer first and then overlay the active view, ensuring the input box remains visible and editable under the status indicator.
- Added unit tests in `tui/src/bottom_pane/mod.rs` to verify input is forwarded during task execution and that the status indicator overlay is removed upon task completion.

**How it works**  
During LLM streaming or tool execution, the `StatusIndicatorView` remains active as an overlay. The modified event handler detects this overlay and forwards user key events to the underlying `ChatComposer` without dismissing the overlay. On task completion (`set_task_running(false)`), the overlay is automatically removed (via `should_hide_when_task_is_done`), returning to the normal input-only view.

## Notes

- Look at the ChatComposer and streaming loop in `tui/src/bottom_pane/chat_composer.rs` for input and stream handling.
- Ensure event loop in `app.rs` multiplexes between agent stream events and user input events without blocking.
- Consider locking or queuing tool-use messages to guarantee prompt tool-output pairing.
