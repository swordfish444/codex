+++
id = "14"
title = "AI‑Generated Approval Predicate Suggestions"
status = "Not started"
dependencies = "01,04,10,12,16,17"
last_updated = "2025-06-25T01:40:09.511783"
+++

# Task 14: AI‑Generated Approval Predicate Suggestions

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

When a shell command is not auto-approved, the approval prompt should include 1–3 AI-generated approval predicates. Each suggestion is a time-limited Python predicate snippet plus an explanation of the full set of permissions it would grant. Users can pick one suggestion to append to the session’s approval policy as a broader-scope allow rule.

## Acceptance Criteria

- When a command is not auto-approved, show up to 3 suggested predicates inline in the TUI approval dialog.
- Each suggestion consists of:
  - A Python code snippet defining a predicate function.
  - An AI-generated explanation of exactly what permissions or scope that predicate grants.
  - A TTL or expiration timestamp indicating how long it will remain active.
- Users can select one suggestion to append to the session’s list of approval predicates.
- Predicates are stored in session state (in-memory) for the duration of the session.
- Provide a slash/CLI command (`/inspect-approval-predicates`) to list current predicates, their code, explanations, and timeouts.
- Support headless and interactive modes equally.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Reuse the existing AI reasoning engine to generate predicate suggestions.
- Represent predicates as Python functions returning a boolean.
- Ensure that expiration is enforced and stale predicates are ignored.
- Integrate the new `/inspect-approval-predicates` command into both the TUI and Exec CLI.
