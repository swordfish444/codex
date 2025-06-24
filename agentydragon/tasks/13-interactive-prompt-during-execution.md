# Task 13: Interactive Prompting and Commands While Executing

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

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
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Look at the ChatComposer and streaming loop in `tui/src/bottom_pane/chat_composer.rs` for input and stream handling.
- Ensure event loop in `app.rs` multiplexes between agent stream events and user input events without blocking.
- Consider locking or queuing tool-use messages to guarantee prompt tool-output pairing.