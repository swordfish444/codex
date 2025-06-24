# Task 12: Runtime Internet Connection Toggle

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Allow users to enable or disable internet access at runtime within their container/sandbox session.

## Acceptance Criteria

- Slash command or CLI subcommand (`/toggle-network <on|off>`) to turn internet on or off immediately.
- Persist network state in session metadata so that resuming a session restores the last setting.
- Enforce the new network policy dynamically: block or allow outbound network connections without restarting the agent.
- Reflect the current network status in the CLI prompt or shell title (e.g. 🌐/🚫).
- Work across supported platforms (Linux sandbox, macOS Seatbelt, Windows) using appropriate sandbox APIs.
- Include unit and integration tests to verify network toggle behavior and persistence.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Reuse the existing sandbox network-disable mechanism (`CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR`) for toggling.
- On Linux, this may involve updating Landlock or seccomp rules at runtime.
- On macOS, interact with the Seatbelt profile; consider session restart if necessary.
- When persisting state, store a `network_enabled: bool` flag in the session JSON.