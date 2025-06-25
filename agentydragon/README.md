# agentydragon

This file documents the changes introduced on the `agentydragon` branch
(off the `main` branch) of the codex repository.

## codex-rs: session resume and playback
- Added `session` subcommand to the CLI (`codex session <UUID>`) to resume TUI sessions by UUID.
- Integrated the `uuid` crate for session identifiers.
- Updated TUI (`codex-rs/tui`) to respect and replay previous session transcripts:
  - Methods: `set_session_id`, `session_id`, `replay_items`.
  - Load rollouts from `sessions/rollout-<UUID>.jsonl`.
- Printed resume command on exit: `codex session <UUID>`.

## codex-core enhancements
- Exposed core model types: `ContentItem`, `ReasoningItemReasoningSummary`, `ResponseItem`.
- Added `composer_max_rows` setting (with serde default) to TUI configuration.

## Dependency updates
- Added `uuid` crate to `codex-rs/cli` and `codex-rs/tui`.

## Pre-commit config changes
- Configured Rust build hook in `.pre-commit-config.yaml` to fail on warnings by setting `RUSTFLAGS="-D warnings"`.

## codex-rs/tui: Undo feedback decision with Esc key
- Pressing `Esc` in feedback-entry mode now cancels feedback entry and returns to the select menu, preserving the partially entered feedback text.
- Added a unit test for the ESC cancellation behavior in `tui/src/user_approval_widget.rs`.

## codex-rs/tui: restore inline mount DSL and slash-command dispatch
- Reintroduced logic in `ChatComposer` to dispatch `AppEvent::InlineMountAdd` and `AppEvent::InlineMountRemove` when `/mount-add` or `/mount-remove` is entered with inline arguments.
- Restored dispatch of `AppEvent::DispatchCommand` for slash commands selected via the command popup, including proper cleanup of the composer input.

## codex-rs/tui: slash-command `/edit-prompt` opens external editor
- Fixed slash-command `/edit-prompt` to invoke the configured external editor for prompt drafting (in addition to Ctrl+E).

## codex-rs/tui: display context remaining percentage
  - Added module `tui/src/context.rs` with heuristics (`approximate_tokens_used`, `max_tokens_for_model`, `calculate_context_percent_remaining`).
  - Updated `ChatWidget` and `ChatComposer::render_ref` to track history items and render `<N>% context left` indicator with color thresholds.
  - Added unit tests in `tui/tests/context_percent.rs` for token counting and percent formatting boundary conditions.

## codex-rs/tui: compact Markdown rendering option
  - Added `markdown_compact` config flag under UI settings to collapse heading-content spacing when enabled.
  - When enabled, headings render immediately adjacent to content with no blank line between them.
  - Updated Markdown rendering in chat UI and logs to honor compact mode globally (diffs, docs, help messages).
  - Added unit tests covering H1–H6 heading spacing for both compact and default modes.
## codex-rs: document MCP servers example in README
- Added an inline TOML snippet under “Model Context Protocol Support” in `codex-rs/README.md` showing how to configure external `mcp_servers` entries in `~/.codex/config.toml`.
- Documented `codex mcp` behavior: JSON-RPC over stdin/stdout, optional sandbox, no ephemeral container, default `codex` tool schema, and example ListTools/CallTool schema.

## Documentation tasks

Tasks live under `agentydragon/tasks/` as individual Markdown files. Please update each task’s **Status** and **Implementation** sections in place rather than maintaining a static list here.

### Branch & Worktree Workflow

- **Branch convention**: work on each task in its own branch named `agentydragon-<task-id>-<task-slug>`, to avoid refname conflicts.
- **Worktree helper**: in `agentydragon/tasks/`, run:
-
-   ```sh
-   # Accept a full slug (NN-slug) or two-digit task ID (NN), optionally multiple; --tmux opens each in its own tmux pane and auto-commits each task as its Developer agent finishes:
-   agentydragon/tools/create_task_worktree.py [--agent] [--tmux] [--skip-presubmit] <task-slug|NN> [<task-slug|NN>...]
-   ```
-
-  Without `--agent`, this creates or reuses a worktree at
-  `agentydragon/tasks/.worktrees/<task-id>-<task-slug>` off the `agentydragon` branch.
-  Internally, the helper uses CoW hydration instead of a normal checkout: it registers the worktree with `git worktree add --no-checkout`, then performs a filesystem-level reflink
-  of all files (macOS: `cp -cRp`; Linux: `cp --reflink=auto`), falling back to `rsync` if reflinks aren’t supported. This makes new worktrees appear nearly instantly on supported filesystems while
-  preserving untracked files.
  -  With `--agent`, after setting up a new worktree it runs presubmit pre-commit checks (aborting with a clear message on failure unless `--skip-presubmit` is passed), then launches the Developer Codex agent (using `prompts/developer.md` and the task file).
  -  After the Developer agent exits, if the task’s **Status** is set to `Done`, it automatically runs the Commit agent helper to stage fixes and commit the work.
**Commit agent helper**: in `agentydragon/tasks/`, run:

```sh
# Generate and apply commit(s) for completed task(s) in their worktrees:
agentydragon/tools/launch_commit_agent.py <task-slug|NN> [<task-slug|NN>...]
```

After the Developer agent finishes and updates the task file, the Commit agent will write the commit message to a temporary file and then commit using that file (`git commit -F`). An external orchestrator can then stage files and run pre-commit hooks as usual. You do not need to run `git commit` manually.

---

*This README was autogenerated to summarize changes on the `agentydragon` branch.*
