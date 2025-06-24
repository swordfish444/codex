# AGENTS.md

**Agents:** Please update `agentydragon/README.md` with a brief summary of your work
(features, fixes, refactors, documentation, etc.) whenever you make changes to this repository. For work on tasks in `agentydragon/tasks/`, update the corresponding task Markdown’s **Status** and **Implementation** sections in place rather than maintaining a central task list.
This serves as a branch-level changelog that the AI assistant will include in its context.

# Rust/codex-rs

In the codex-rs folder where the rust code lives:

- Never add or modify any code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR`. You operate in a sandbox where `CODEX_SANDBOX_NETWORK_DISABLED=1` will be set whenever you use the `shell` tool. Any existing code that uses `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` was authored with this fact in mind. It is often used to early exit out of tests that the author knew you would not be able to run given your sandbox limitations.
