+++
id = "25"
title = "Guard Against Missing Tool Output in Rust Server Sequencing"
status = "Not started"
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

**How it was implemented**  
- Introduce a pending-invocation registry (`HashMap<InvocationId, PendingState>`) in the Rust message pipeline.
- Modify `handle_user_message` and `handle_model_event` in the broker to check for unresolved pending invocations and enqueue incoming events accordingly.
- On receiving the corresponding tool output or tool abort event, dequeue and dispatch any buffered messages in order.
- Implement a timeout or explicit cancel path to avoid stuck invocations in case of unresponsive tools.
- Extend the Rust test suite (e.g. in `broker/tests/`) with scenarios covering normal, aborted, and concurrent messages.

## Notes

- Mirror the JS implementation guard patterns for consistency across backends.
- Provide clear logging at the debug level to trace sequencing steps during development.
