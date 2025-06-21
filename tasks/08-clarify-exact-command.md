# Task 08: Clarify “Exact Command” Wording in Approval Prompts

## Goal
Refine the approval dialog wording to clearly explain what “this exact command” means: whether it matches the exact command string, the binary invocation, or the tool category.

## Acceptance Criteria
- Update the approval modal description in `tui/src/bottom_pane/approval_modal_view.rs` to specify that it matches the literal command line string.
- Adjust config docs in `codex-rs/docs/protocol_v1.md` and `config.md` to mirror the clarified definition.

## Notes
- This is purely a documentation and UI-text change; no logic modification required.