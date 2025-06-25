+++
id = "25"
title = "Guard Against Missing Tool Output in Rust Server Sequencing"
status = "In progress"
dependencies = "03,06,08,13,15,32,18,19,22,23"
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Prevent out-of-order chat messages and missing tool output errors when user input interrupts tool execution in the Rust backend.

## Goal
Ensure the Rust server implementation sequences tool output and chat messages correctly. Add synchronization logic so that an in-flight tool invocation either completes or is cancelled before new messages are processed, avoiding "No tool output found" invalid_request errors.

## Acceptance Criteria

- The Rust message broker must detect pending tool invocations and pause delivery of subsequent user or model messages until the tool result or cancellation is handled.
- No panic or 400 Bad Request errors should occur due to missing tool output in edge cases of interrupted rollouts or mid-stream user input.
- Add Rust integration tests simulating tool invocation interruption and user message interleaving, verifying correct ordering and delivery.

## Implementation

We will implement the following high-level plan:

- Locate where the ChatCompletion request messages array is built in Rust:
  the `stream_chat_completions` function in `codex-rs/core/src/chat_completions.rs`.
- In that loop, track pending tool invocations by their call IDs when encountering `ResponseItem::FunctionCall` entries.
- Buffer any subsequent `ResponseItem::Message { role: "user" }` or new turn inputs until the matching `ResponseItem::FunctionCallOutput` (tool result) appears.
- Once the tool output is seen, flush buffered user messages in order immediately before continuing to build the next API call.
- Add tests under `codex-rs/core/tests/` (e.g. `guard_tool_output_sequencing.rs`) that exercise interleaved input sequences:
  - A user message mid-rollout before tool output, ensuring it is delayed until after the tool result.
  - Normal flow where no buffering is needed.
  - Cancellation paths (no tool output) inserting a fake "Tool cancelled" tool message and flushing buffered messages.

## Notes

- Mirror the JS implementation guard patterns for consistency across backends.
- Provide clear logging at the debug level to trace sequencing steps during development.
