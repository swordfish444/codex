# Task 02: Granular Auto-Approval Predicates

## Goal
Let users configure one or more scripts in `config.toml` that examine each proposed shell command and output exactly one of:

- `continue`        => auto-approve and proceed under the sandbox
- `deny`            => auto-reject (skip sandbox and do not run the command)
- `user-confirm`    => pause execution and open the interactive approval dialog for manual decision

If the script exits non-zero or prints anything else, default to `user-confirm`.

## Acceptance Criteria
- New `[[auto_allow]]` table in `config.toml` supporting one or more `script = "..."` entries.
- Before running any shell/subprocess, Codex invokes each configured script in order, passing the candidate command as an argument.
- If a script prints `continue`/`deny`/`user-confirm`, take that action and skip remaining scripts.
- If all scripts return non-zero or invalid output, pause for manual approval (existing logic).

## Notes
- This pairs with the existing `approval_policy = "unless-allow-listed"` but adds custom logic before prompting.