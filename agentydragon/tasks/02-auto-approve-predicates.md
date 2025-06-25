# Task 02: Granular Auto-Approval Predicates

> *This task is specific to codex-rs.*

## Status

**General Status**: In progress  
**Summary**: Implementation underway; populating Implementation section and coding auto-approval predicates.

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
**Planned Implementation**  
1. Extend `ConfigToml` and `ConfigOverrides` to parse a new `[[auto_allow]]` table with `script` entries.  
2. Propagate `auto_allow` scripts from `Config` into `Session`.  
3. Add a helper function `get_auto_allow_vote` that invokes a single script, treats non-zero exits as no-opinion (logging a warning), and parses stdout to a vote.  
4. Update the exec pipeline (`handle_container_exec_with_params`) to, before safety checks, iterate through `auto_allow` scripts and handle all vote outcomes:
   - On `deny`, auto-reject.
   - On `allow`, auto-approve under sandbox.
   - On script errors or unrecognized outputs, immediately prompt the user (via `request_command_approval`) with a reason string describing the error/output, then run or reject based on their decision.
   - On `no-opinion`, fall through to the next script.
6. Write async unit tests for `get_auto_allow_vote`, covering allow, deny, no-opinion, and error-exit cases.  
7. Update documentation (`config.md`) to document the new auto-approval predicates feature.

## Notes
- This pairs with the existing `approval_policy = "unless-allow-listed"` but now ensures script errors or unexpected outputs trigger a targeted manual approval prompt with context.
