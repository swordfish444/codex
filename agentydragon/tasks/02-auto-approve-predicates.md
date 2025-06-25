+++
id = "02"
title = "Granular Auto-Approval Predicates"
status = "Merged"
dependencies = ""
last_updated = "2025-06-25T01:40:09.503983"
+++

# Task 02: Granular Auto-Approval Predicates

> *This task is specific to codex-rs.*

## Status

**General Status**: Merged  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

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

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes
- This pairs with the existing `approval_policy = "unless-allow-listed"` but adds custom logic before prompting.
