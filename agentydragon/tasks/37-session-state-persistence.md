+++
id = "37"
title = "Session State Persistence and Debug Instrumentation"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T23:00:00.000000"
+++

## Summary
Persist session runtime state and capture raw request/response data and supplemental metadata to a session-specific directory.

## Goal
Collect and persist all relevant session state (beyond the rollout transcript) in a dedicated directory under `.codex/sessions/<UUID>/`, to aid debugging and allow post-mortem analysis.

## Acceptance Criteria

- All session data (transcript, logs, raw OpenAI API requests/responses, approval events, and other runtime metadata) is written under `.codex/sessions/<session_id>/`.
- Existing rollout transcript continues to be written to `sessions/rollout-<UUID>.jsonl`, now moved or linked into the session directory.
- Logging configuration respects `--debug-log` and writes to the session directory when set to a relative path.
- A selector flag (e.g. `--persist-session`) enables or disables writing persistent state.
- No change to default behavior when persistence is disabled (i.e. backward compatibility).
- Minimal integration test or manual verification steps demonstrate that files appear correctly and no extraneous error logs occur.

## Implementation

**How it was implemented**  
- Add a new CLI flag `--persist-session` to the TUI and server binaries to enable session persistence.
- Compute a session directory under `$CODEX_HOME/sessions/<UUID>/`, create it at startup when persistence is enabled.
- After initializing the rollout file (`rollout-<UUID>.jsonl`), move or symlink it into the session directory.
- Configure tracing subscriber file layer and `--debug-log` default path to write logs into the same session directory (e.g. `session.log`).
- Instrument the OpenAI HTTP client layer to dump raw request and response bodies into `session_oai_raw.log` in that directory.
- In the message sequencing logic, add debug spans to record approval and cancellation events into `session_meta.log`.

**How it works**  
- When `--persist-session` is active, all file outputs (rollout transcript, debug logs, raw API dumps, metadata logs) are collated under a single session directory.
- If disabled (default), writes occur in the existing locations (`rollout-<UUID>.jsonl`, `$CODEX_HOME/log/`), preserving current behavior.

## Notes

- This feature streamlines troubleshooting by co-locating all session artifacts.
- Ensure directory creation and file writes handle permission errors gracefully and fallback cleanly when disabled.
