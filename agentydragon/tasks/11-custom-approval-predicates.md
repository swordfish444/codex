# Task 11: User-Configurable Approval Predicates

> *This task is specific to codex-rs.*

## Status

**General Status**: Merged  
**Summary**: Implemented custom approval predicates feature: configuration parsing, predicate invocation logic, tests, and documentation.

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
- Added `approval_predicates` field to `ConfigToml` and `Config` in `codex_core::config`, supporting a `python_predicate_binary: PathBuf` and an implicit `never_expire = true`.
- Hooked into the command-approval code path in `codex_core::safety` to invoke each configured predicate executable before showing the approval prompt. Predicates are launched via `std::process::Command` with context passed in environment variables (`CODEX_SESSION_ID`, `CODEX_CONTAINER_CWD`, `CODEX_HOST_CWD`, `CODEX_COMMAND`).
- Parsed each predicate’s stdout for exactly `allow`, `deny`, or `ask`, short-circuiting on `allow` or `deny` (auto-approve/auto-reject) and treating failures or unexpected output as `ask` to continue to the next predicate.
- Wrote unit tests for configuration parsing and predicate-invocation behavior, covering exit-code and output edge cases, plus integration tests verifying end-to-end approval decisions.
- Updated `config.md` to document the `[[approval_predicates]]` table syntax, default semantics, and runtime behavior.

**How it works**  
When a shell command requires approval, Codex iterates over each entry in `[[approval_predicates]]` in order. For each predicate:
- Launch the configured binary with session context in its environment.
- If it exits successfully and writes `allow`, Codex auto-approves and skips remaining predicates.
- If it writes `deny`, Codex auto-rejects and skips remaining predicates.
- Otherwise (writes `ask`, fails, or emits unexpected output), Codex moves to the next predicate or falls back to the manual approval dialog if none return `allow` or `deny`.
This mechanism lets users automate approval decisions via custom Python scripts while retaining manual control when predicates defer.

## Notes

- Consider passing context via environment variables (e.g. `CODEX_SESSION_ID`, `CODEX_CONTAINER_CWD`, `CODEX_HOST_CWD`, `CODEX_COMMAND`).
- Reuse invocation logic from the auto-approval predicates feature (Task 02).
- **Motivating example**: auto-approve `pre-commit run --files <any number of space-separated files>`.
- **Motivating example**: auto-approve any `git` command (e.g. `git add`, `git commit`, `git push`, `git status`, etc.) provided its repository root is under `<directory>`, correctly handling common flags and safe invocation modes.
- **Motivating example**: auto-approve any shell pipeline composed out of `<these known-safe commands>` operating on `<known-safe files>` with `<known-safe params>`, using a general pipeline parser to ensure safety—a nontrivial example of predicate logic.
