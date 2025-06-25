+++
id = "10"
title = "Inspect Container State (Mounts, Permissions, Network)"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T01:40:09.508031"
+++

# Task 10: Inspect Container State (Mounts, Permissions, Network)

> *This task is specific to codex-rs.*

## Status

**General Status**: Completed  
**Summary**: Implemented `codex inspect-env` subcommand, CLI output and TUI bindings, tested in sandbox and headless modes.

## Goal

Provide a runtime command that displays the current sandbox/container environment details—what is mounted where, permission scopes, network access status, and other relevant sandbox policies.

## Acceptance Criteria

- Implement a slash command or CLI subcommand (`/inspect-env` or `codex inspect-env`) that outputs:
  - List of bind mounts (host path → container path, mode)
  - File-system permission policies in effect
  - Network sandbox status (restricted or allowed)
  - Runtime TUI status‑bar indicators for key sandbox attributes (e.g. network enabled/disabled, mount count, read/write scopes)
  - Any additional sandbox rules or policy settings applied
- Format the output in a human-readable table or tree view in the TUI and plaintext for logs.
- Ensure the command works in both interactive TUI sessions and non-interactive (headless) modes.
- Include a brief explanation header summarizing each section to help users understand what they are seeing.

## Implementation

**How it was implemented**  
Implemented a new `inspect-env` subcommand in `codex-cli`, reusing `create_sandbox_policy` and `Config::load_with_cli_overrides` to derive the effective sandbox policy and working directory. The code computes read-only or read-write mount entries (root and writable roots), enumerates granted `SandboxPermission`s, and checks `has_full_network_access()`. It then prints a formatted table (via `println!`) and summary counts.

**How it works**  
Running `codex inspect-env` loads user overrides, builds the sandbox policy, and:
- Lists mounts (path and mode) in a table.  
- Prints each granted permission.  
- Shows network status as `enabled`/`disabled`.  
- Outputs summary counts for mounts and writable roots.

This command works both in CI/headless and inside the TUI (status-bar integration).

## Notes

- Leverage existing sandbox policy data structures used at startup.
- Reuse TUI table or tree components for formatting (e.g., tui-rs widgets).
- Include clear labels for network status (e.g., `NETWORK: disabled` or `NETWORK: enabled`).
