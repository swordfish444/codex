# AGENTS.md

**Agents:**
- Update `agentydragon/README.md` with a brief summary of your work (features, fixes, refactors, documentation, etc.) whenever you make changes to this repository.
- Read `agentydragon/README.md` for the branch-level changelog and guidelines on task conventions.
- For work on tasks, “add task” means creating a new Markdown file under `agentydragon/tasks/` using `task-template.md`:
  - Name it with a two-digit prefix and kebab-case slug (e.g. `14-new-feature.md`).
  - Fill in the **Status**, **Goal**, **Acceptance Criteria**, and **Implementation** sections.
- No central task list should be maintained. The AI assistant will include these branch-level notes in its context.

# Rust/codex-rs

In the codex-rs folder where the rust code lives:

- Never add or modify any code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR`. You operate in a sandbox where `CODEX_SANDBOX_NETWORK_DISABLED=1` will be set whenever you use the `shell` tool. Any existing code that uses `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` was authored with this fact in mind. It is often used to early exit out of tests that the author knew you would not be able to run given your sandbox limitations.
