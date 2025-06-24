# Task 11: User-Configurable Approval Predicates

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Allow users to plug in an external executable that makes approval decisions for shell commands based on session context.

## Acceptance Criteria

- Support a new `[[approval_predicates]]` section in `config.toml` for Python-based predicates, each with a `python_predicate_binary = "..."` field (pointing to the predicate executable) and an implicit `never_expire = true` setting.
- Before prompting the user, invoke each configured predicate in order, passing the following (via CLI args or env vars):
  - Session ID
  - Container working directory (CWD)
  - Host working directory (CWD)
  - Candidate shell command string
- The predicate must print exactly one of `allow`, `deny`, or `ask` on stdout:
  - `allow`  → auto-approve and skip remaining predicates
  - `deny`   → auto-reject and skip remaining predicates
  - `ask`    → open the standard approval dialog and skip remaining predicates
- If a predicate exits non-zero or outputs anything else, treat it as `ask` and continue to the next predicate.
- Write unit and integration tests covering typical and edge-case predicate behavior.
- Document configuration syntax and behavior in the top-level config docs (`config.md`).

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Consider passing context via environment variables (e.g. `CODEX_SESSION_ID`, `CODEX_CONTAINER_CWD`, `CODEX_HOST_CWD`, `CODEX_COMMAND`).
- Reuse invocation logic from the auto-approval predicates feature (Task 02).