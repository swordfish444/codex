# codex-rs: Changes between HEAD and main

This document summarizes new and removed features, configuration options,
and behavioral changes in the `codex-rs` workspace between the `main`
branch and the current `HEAD`. Only additions/deletions (not unmodified
code) are listed, with examples of usage and configuration.

---

## CLI Enhancements

### Build & Install from Source

```shell
cargo install --path cli --locked
# install system-wide:
sudo cargo install --path cli --locked --root /usr/local
```

### New `codex config` Subcommand

Manage your `~/.codex/config.toml` directly without manually editing:

```shell
codex config edit            # open config in $EDITOR (or vi)
codex config set KEY VALUE   # set a TOML literal, e.g. tui.auto_mount_repo true
```

### New `codex inspect-env` Command

Inspect the sandbox/container environment (mounts, permissions, network):

```shell
codex inspect-env --full-auto
codex inspect-env -s network=disable -s mount=/mydir=rw
```

### Resume TUI Sessions by UUID

```shell
codex session <SESSION_UUID>
```

### MCP Server (JSON‑RPC) Support

Launch Codex as an MCP _server_ over stdin/stdout and speak the
Model Context Protocol (JSON-RPC):

```shell
npx @modelcontextprotocol/inspector codex mcp
```

#### Sample JSON‑RPC Interaction

```jsonc
// ListTools request
{ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }

// CallTool request
{ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
  "params": { "name": "codex", "arguments": { "prompt": "Hello" } }
}

// CallTool response (abbreviated)
{ "jsonrpc": "2.0", "id": 2, "result": {
    "content": [ { "type": "text", "text": "Hi there", "annotations": null } ],
    "is_error": false
}}
```

---

## Configuration Changes

### `auto_allow` Predicate Scripts

Automatically approve or deny shell commands via custom scripts:

```toml
[[auto_allow]]
script = "/path/to/approve_predicate.sh"
[[auto_allow]]
script = "my_predicate --flag"
```

Vote resolution:
- A `deny` vote aborts execution.
- An `allow` vote auto-approves.
- Otherwise falls back to manual approval prompt.

### `base_instructions_override`

Override or disable the built-in system prompt (`prompt.md`):

```bash
export CODEX_BASE_INSTRUCTIONS_FILE=custom_prompt.md   # use custom prompt
export CODEX_BASE_INSTRUCTIONS_FILE=""             # disable base prompt
```

### TUI Configuration Options

In `~/.codex/config.toml`, under the `[tui]` table:

```toml
editor          = "${VISUAL:-${EDITOR:-nvim}}"  # external editor for prompt
message_spacing = true                           # insert blank line between messages
sender_break_line = true                         # sender label on its own line
```

---

## Core Library Updates

### System Prompt Composition Customization

System messages now combine:
1. Built-in prompt (`prompt.md`),
2. User instructions (`AGENTS.md`/`instructions.md`),
3. `apply-patch` tool instructions (for GPT-4.1),
4. User command/prompt.

Controlled via `CODEX_BASE_INSTRUCTIONS_FILE`.

### Chat Completions Tool Call Buffering

User turns emitted during an in-flight tool invocation are buffered
and flushed after the tool result, preventing interleaved messages.

### SandboxPolicy API Extensions

```rust
policy.allow_disk_write_folder("/path/to/folder".into());
policy.revoke_disk_write_folder("/path/to/folder");
```

### Auto‑Approval Predicate Engine

```rust
use codex_core::safety::{evaluate_auto_allow_predicates, AutoAllowVote};
let vote = evaluate_auto_allow_predicates(&cmd, &config.auto_allow);
match vote {
    AutoAllowVote::Allow => /* auto-approve */, 
    AutoAllowVote::Deny => /* reject */, 
    AutoAllowVote::NoOpinion => /* prompt user */, 
}
```

---

## TUI Improvements

### Double Ctrl+D Exit Confirmation

Prevent accidental exits by requiring two Ctrl+D within a timeout:

```rust
use codex_tui::confirm_ctrl_d::ConfirmCtrlD;
let mut confirm = ConfirmCtrlD::new(require_double, timeout_secs);
// confirm.handle(now) returns true to exit, false to prompt confirmation
```

### Markdown & Header Compact Rendering

New rendering options (code-level) for more compact chat layout:
- `markdown_compact`
- `header_compact`

---

## Documentation & Tests

- `codex-rs/config.md`, `codex-rs/README.md`, `core/README.md` updated with examples.
- New `core/init.md` guidance for generating `AGENTS.md` templates.
- Added tests for `codex config`, `ConfirmCtrlD`, and `evaluate_auto_allow_predicates`.
