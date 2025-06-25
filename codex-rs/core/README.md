# codex-core

This crate implements the business logic for Codex. It is designed to be used by the various Codex UIs written in Rust.

## System Prompt Composition

Codex composes the initial system message that seeds every chat completion turn as follows:

1. Load the built-in system prompt from `prompt.md` (unless disabled).
2. If the `CODEX_BASE_INSTRUCTIONS_FILE` env var is set, use that file instead of `prompt.md`.
3. Append any user instructions (e.g. from `instructions.md` and merged `AGENTS.md`).
4. Append the apply-patch tool instructions when using GPT-4.1 models.
5. Finally, the user's command or prompt is sent as the first user message.

The base instructions behavior can be customized with `CODEX_BASE_INSTRUCTIONS_FILE`:

- If unset, the built-in prompt (`prompt.md`) is used.
- If set to a valid file path, that file's contents will be used instead (failure to read will abort).
- If set to an empty string or `-`, no system prompt will be sent.

For implementation details, see `client_common.rs` and `project_doc.rs`.

Though for non-Rust UIs, we are also working to define a _protocol_ for talking to Codex. See:

- [Specification](../docs/protocol_v1.md)
- [Rust types](./src/protocol.rs)

You can use the `proto` subcommand using the executable in the [`cli` crate](../cli) to speak the protocol using newline-delimited-JSON over stdin/stdout.
