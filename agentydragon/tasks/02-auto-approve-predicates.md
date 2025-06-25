+++
id = "02"
title = "Granular Auto-Approval Predicates"
status = "Done"
dependencies = "11" # Rationale: depends on Task 11 for user-configurable approval predicates
last_updated = "2025-06-25T10:48:30.000000"
+++

# Task 02: Granular Auto-Approval Predicates

> *This task is specific to codex-rs.*

## Status
**General Status**: Done  
**Summary**: Added granular auto-approval predicates: configuration parsing, predicate evaluation, integration, documentation, and tests.

## Goal
Let users configure one or more scripts in `config.toml` that examine each proposed shell command and return exactly one of:

- `deny`        => auto-reject (skip sandbox and do not run the command)
- `allow`       => auto-approve and proceed under the sandbox
- `no-opinion`  => no opinion (neither approve nor reject)

Multiple scripts cast votes: if any script returns `deny`, the command is denied; otherwise if any script returns `allow`, the command is allowed; otherwise (all scripts return `no-opinion` or exit non-zero), pause for manual approval (existing logic).

## Acceptance Criteria
- New `[[auto_allow]]` table in `config.toml` supporting one or more `script = "..."` entries.
- Before running any shell/subprocess, Codex invokes each configured script in order, passing the candidate command as an argument.
- If a script returns `deny` or `allow`, immediately take that vote and skip remaining scripts.
- After all scripts complete with only `no-opinion` results or errors, pause for manual approval (existing logic).

 - Spawn each predicate script with the full command as its only argument.
 - Parse stdout (case-insensitive) expecting `deny`, `allow`, or `no-opinion`, treating errors or unknown output as `NoOpinion`.
 - Short-circuit on the first `Deny` or `Allow` vote.
 - A `Deny` vote aborts execution.
 - An `Allow` vote skips prompting and proceeds under sandbox.
 - All `NoOpinion` votes fall back to existing approval logic.

## Implementation
-- Added `auto_allow: Vec<AutoAllowPredicate>` to `ConfigToml`, `ConfigProfile`, and `Config` to parse `[[auto_allow]]` entries from `config.toml`.
-- Defined `AutoAllowPredicate { script: String }` and `AutoAllowVote { Allow, Deny, NoOpinion }` in `core::safety`.
-- Implemented `evaluate_auto_allow_predicates` in `core::safety` to spawn each script with the candidate command, parse its stdout vote, and short-circuit on `Deny` or `Allow`.
-- Integrated `evaluate_auto_allow_predicates` into the shell execution path in `core::codex`, aborting on `Deny`, auto-approving on `Allow`, and falling back to manual or policy-based approval on `NoOpinion`.
-- Updated `config.md` to document the `[[auto_allow]]` table syntax and behavior.
-- Added comprehensive unit tests covering vote parsing, error propagation, short-circuit behavior, and end-to-end predicate functionality.
## Notes
- This pairs with the existing `approval_policy = "unless-allow-listed"` but adds custom logic before prompting.
