+++
id = "24"
title = "Guard Against Missing Tool Output in JS Server Sequencing"
status = "Not started"
dependencies = "03,06,08,13,15,32,18,19,22,23"
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Prevent out-of-order chat messages and missing tool outputs when user input interrupts tool execution in the JS backend.

## Goal
Ensure the JS server never emits a user or model message before the corresponding tool output has been delivered. Add sequencing guards to the message dispatcher so that aborted rollouts or interleaved user messages cannot cause "No tool output found" errors.

## Acceptance Criteria

- When a tool invocation is interrupted or user sends a message mid-rollout, the JS server buffers subsequent messages until the tool output event arrives or the invocation is explicitly cancelled.
- The server must never log or emit an error like "No tool output found for local shell call" due to sequencing mismatch.
- Add automated tests simulating mid-rollout user interrupts in the JS test suite, verifying correct buffering and eventual message delivery or cancellation.

## Implementation

**How it was implemented**  
- In the JS message dispatcher, track pending tool invocations by ID and delay processing of new chat messages until the pending invocation resolves (success, failure, or cancel).
- Add a guard in the `handleUserMessage` path to check for unresolved tool IDs before appending user content; if pending, queue the message.
- On receiving `toolOutput` or `toolError` for an invocation ID, flush any queued messages in order.
- Implement explicit cancellation paths so that if a tool invocation is abandoned, queued messages still flow after cancellation confirmation.
- Add unit and integration tests in the JS test harness to cover normal, aborted, and concurrent message scenarios.

## Notes

- This change prevents 400 Bad Request errors from tool retries where the model requests a tool before the output is streamed.
- Keep diagnostic logs around sequencing logic for troubleshooting but avoid spamming on normal race cases.
