# Task 10: Inspect Container State (Mounts, Permissions, Network)

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Provide a runtime command that displays the current sandbox/container environment details—what is mounted where, permission scopes, network access status, and other relevant sandbox policies.

## Acceptance Criteria

- Implement a slash command or CLI subcommand (`/inspect-env` or `codex inspect-env`) that outputs:
  - List of bind mounts (host path → container path, mode)
  - File-system permission policies in effect
  - Network sandbox status (restricted or allowed)
  - Any additional sandbox rules or policy settings applied
- Format the output in a human-readable table or tree view in the TUI and plaintext for logs.
- Ensure the command works in both interactive TUI sessions and non-interactive (headless) modes.
- Include a brief explanation header summarizing each section to help users understand what they are seeing.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Leverage existing sandbox policy data structures used at startup.
- Reuse TUI table or tree components for formatting (e.g., tui-rs widgets).
- Include clear labels for network status (e.g., `NETWORK: disabled` or `NETWORK: enabled`).