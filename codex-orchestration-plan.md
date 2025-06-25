# Codex Orchestration Framework: Plan & Open Questions

This document collects the high‑level architecture, planned features, and unresolved design decisions for the proposed **codex-agents** orchestration framework.

## 1. Architecture & Core Components

- **XDG‑compliant configuration & state**
  - Repo‑local overrides: `<repo>/.codex-agent/config.toml`
  - User‑wide config: `$XDG_CONFIG_HOME/codex-agents/config.toml`
  - Global task registry: `$XDG_DATA_HOME/codex-agents/tasks.json`

- **CLI & optional TUI**
  - `codex-agent init` → bootstrap repo (copy prompts, create directories)
  - `codex-agent status [--tui]` → show global and per‑repo task/merge status
  - `codex-agent config` → inspect or edit effective config
  - `codex-agent agents` → view per‑agent instruction overrides

- **Task management (`codex-agent task`)**
  - `add`, `list`, `edit`, `worktree add|remove`, `validate`, `review`, `complete`
  - Interactive AI Q&A flow for `task add` to auto‑populate slug, goal, dependencies, and stub file

- **Worktree hydration**
  - OS‑aware reflink: macOS `cp -cRp`, Linux `cp --reflink=auto`, fallback to `rsync`
  - COW setup via `git worktree add --no-checkout` + hydration step

- **Merge & Conflict Resolver (`codex-agent merge`)**
  - `merge check` → dry‑run merge in temp worktree
  - `merge resolve` → AI‑driven conflict resolution or explicit bail-out
  - `merge rebase` → manual rebase entrypoint

- **Code Validator (`codex-agent task validate|review`)**
  - Run linters/tests, then invoke Validator agent prompt
  - Enforce configurable policies (doc coverage, style rules, test thresholds)

- **Project Manager (`codex-agent manager`)**
  - Wave planning, parallel launch commands, live monitoring of worktrees

## 2. Phased Roadmap

Phase | Deliverables
:----:|:--------------------------------------------------------------------------------------
1     | XDG config + global `tasks.json` + basic `task list|add|worktree` CLI
2     | Merge check & conflict-resolver prompt + `merge check|resolve` commands
3     | Validator agent integration + `task validate|review`
4     | Project Manager planning & launching (`manager plan|launch|monitor`)
5     | Interactive `task add` QA loop + per-agent instruction overrides
6     | TUI mode for `status` + live dashboard
7     | Polishing docs, tests, packaging, and PyPI release

## 3. Open Questions & Design Decisions

1. **Global registry schema**
   - What additional fields should `tasks.json` track? (e.g. priority, owner, labels)

2. **Config file format & schema**
   - TOML vs YAML vs JSON for `config.toml`?
   - Which policy keys to expose for Validator and Resolver agents?

3. **Per‑agent instruction overrides**
   - How to structure override files (`validator.toml`, `conflict-resolver.toml`, etc.)?
   - Should we fallback to AGENTS‑style instruction files in the repo root if present?

4. **CLI command names & flags**
   - Confirm subcommand verbs (`merge resolve` vs `task rebase`, `task validate` vs `task lint`)
   - Standardize flags for interactive vs non‑interactive modes

5. **Conflict Resolver scope**
   - Auto‑resolve only trivial hunks, or attempt full rebase‑based AI resolution?
   - How and when can the agent “give up” and hand control back to the user?

6. **Validator policies & auto‑fix**
   - Default policy values (max line length, doc coverage %)
   - Should `--auto-fix` let the agent rewrite code, or only report issues?

7. **Interactive Task Creation**
   - Best UX for prompting the user: CLI Q&A loop vs opening an editor with agent instructions?
   - How to capture dependencies and inject them into the new task stub?

8. **Session restore UX**
   - Always on for `codex session <UUID>`, or opt‑in via flag?
   - How to surface restore failures or drift in transcript format?

9. **TUI implementation**
   - Framework choice (curses, Rich, Textual)
   - Auto‑refresh interval and keybindings for actions (open worktree, resolve, validate)

10. **Packaging & distribution**
   - Final PyPI package name (`codex-agents` vs `ai-orchestrator`)
   - Versioning strategy and backwards‑compatibility guarantees

---

_This plan will evolve as we answer these questions and move through the roadmap phases._
