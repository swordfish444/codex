# codex-rs Changelog (HEAD vs main)

This document summarizes the **codex-rs** related changes introduced on the `agentydragon` branch (HEAD) compared to `main`. It covers new features, configuration options, and usage examples.

## 1. Session resume and playback

**Added** a new `session` subcommand to resume TUI sessions by UUID:

```bash
# Start or resume a session by UUID:
codex session 123e4567-e89b-12d3-a456-426614174000
```

The TUI now records your session transcript under:

```
sessions/rollout-<UUID>.jsonl
```

APIs exposed in code (`tui` crate):
- `set_session_id`
- `session_id`
- `replay_items`

On exit, Codex prints a resume reminder:

```text
Resume this session with: codex session 123e4567-e89b-12d3-a456-426614174000
```

## 2. codex-core enhancements

Exposed core model types and added a new TUI configuration setting:

```toml
## config.toml
[core]
# Core model types available programmatically:
# ContentItem, ReasoningItemReasoningSummary, ResponseItem

[tui]
# Maximum number of rows for the composer input area
composer_max_rows = 10
```

## 3. Dependency updates

- Added the `uuid` crate to `codex-rs/cli` and `codex-rs/tui` for stable session identifiers.

## 4. Pre-commit config changes

- Updated `.pre-commit-config.yaml` to fail Rust builds on warnings:

```yaml
-   repo: local
    hooks:
      - id: rust-build
        entry: bash -lc 'cd codex-rs && RUSTFLAGS="-D warnings" cargo build --workspace --locked'
```

## 5. TUI improvements

### 5.1 Undo feedback decision with Esc key

Pressing `Esc` in feedback-entry mode now cancels feedback and returns to the select menu, preserving partially entered text.

```rust
// New unit test in tui/src/user_approval_widget.rs
#[test]
fn esc_cancels_feedback_entry() {
    // ...
}
```

### 5.2 Restore inline mount DSL and slash-command dispatch

Reintroduced support for inline `/mount-add` and `/mount-remove` commands, plus proper slash-command dispatch from the popup:

```text
# Within the composer, type:
/mount-add /path/to/dir
/mount-remove /path/to/dir
```

### 5.3 `/edit-prompt` opens external editor

The `/edit-prompt` slash command now invokes your configured `$EDITOR` for prompt drafting (in addition to `Ctrl+E`):

```text
# In the TUI composer:
/edit-prompt
```

---

*End of codex-rs changelog.*
