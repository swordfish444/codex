# Codex CLI (Rust Implementation)

We provide Codex CLI as a standalone, native executable to ensure a zero-dependency install.

## Installing Codex

Today, the easiest way to install Codex is via `npm`, though we plan to publish Codex to other package managers soon.

```shell
npm i -g @openai/codex@native
codex
```

You can also build and install the Rust-native binary from source:

```shell
cargo install --path cli --locked
```

By default this installs into `$HOME/.cargo/bin`, so make sure that's on your `PATH`. To install system-wide (e.g. to `/usr/local`), run:

```shell
sudo cargo install --path cli --locked --root /usr/local
```

You can also download a platform-specific release directly from our [GitHub Releases](https://github.com/openai/codex/releases).

## What's new in the Rust CLI

While we are [working to close the gap between the TypeScript and Rust implementations of Codex CLI](https://github.com/openai/codex/issues/1262), note that the Rust CLI has a number of features that the TypeScript CLI does not!

### Config

Codex supports a rich set of configuration options. Note that the Rust CLI uses `config.toml` instead of `config.json`. See [`config.md`](./config.md) for details.

### Model Context Protocol Support

Codex CLI functions as an MCP client that can connect to MCP servers on startup. See the [`mcp_servers`](./config.md#mcp_servers) section in the configuration documentation for details.

For example, to configure an external MCP server in your `~/.codex/config.toml`:

```toml
[mcp_servers.server-name]
command = "npx"
args    = ["-y", "mcp-server"]
env     = { "API_KEY" = "value" }
```

It is still experimental, but you can also launch Codex as an MCP _server_ by running `codex mcp`. Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector codex mcp
```

Under the hood, `codex mcp` launches a local MCP server process that communicates over stdin/stdout
using the Model Context Protocol (JSON-RPC messages). It reads `JSONRPCMessage` requests (e.g.
`Initialize`, `ListTools`, `CallTool`) from stdin, handles them, and writes JSONRPCMessage
responses and notifications to stdout. No separate container or VM is spun up and torn down; on
Linux the process is optionally sandboxed via Landlock/seccomp (and on macOS via Seatbelt).
See `codex-rs/mcp-server/src/lib.rs` for the implementation.

By default, the server advertises a single MCP tool named `codex`.  A `ListTools` request
will return this tool along with its input schema (fields: `prompt`, `model`, `profile`,
`cwd`, `approval_policy`, `sandbox_permissions`, `config`).

#### Example: ListTools and CallTool messages

```jsonc
// ListTools request
{ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }

// ListTools response (abbreviated)
{ "jsonrpc": "2.0", "id": 1,
  "result": { "tools": [
    {
      "name": "codex",
      "description": "Run a Codex session. Accepts configuration parameters matching the Codex Config struct.",
      "input_schema": { /* JSON Schema with properties: prompt, model, profile, cwd, approval_policy, sandbox_permissions, config */ }
    }
  ] }
}

// CallTool request to run the 'codex' tool with a prompt
{ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
  "params": { "name": "codex",
              "arguments": { "prompt": "Hello, world!" }
  }
}

// CallTool response (abbreviated)
{ "jsonrpc": "2.0", "id": 2,
  "result": {
    "content": [ { "type": "text", "text": "Hello, world! How can I help?", "annotations": null } ],
    "is_error": false
  }
}
```

Calls to the `codex` tool are handled via JSON-RPC `CallTool` requests by spawning an interactive Codex
session based on the provided parameters and streaming the generated text in the `content` field.

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](./config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS.

### `codex exec` to run Codex programmatially/non-interactively

To run Codex non-interactively, run `codex exec PROMPT` (you can also pass the prompt via `stdin`) and Codex will work on your task until it decides that it is done and exits. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.

### `--cd`/`-C` flag

Sometimes it is not convenient to `cd` to the directory you want Codex to use as the "working root" before running Codex. Fortunately, `codex` supports a `--cd` option so you can specify whatever folder you want. You can confirm that Codex is honoring `--cd` by double-checking the **workdir** it reports in the TUI at the start of a new session.

### `codex config` to manage your configuration file

Codex now provides a built-in `config` subcommand for managing your `config.toml`:

```shell
codex config edit               # open ~/.codex/config.toml in $EDITOR (or vi)
codex config set KEY VALUE      # set a config key to a TOML literal, e.g. tui.auto_mount_repo true
```

Use `codex config --help` for more details.

### Experimenting with the Codex Sandbox

To test to see what happens when a command is run under the sandbox provided by Codex, we provide the following subcommands in Codex CLI:

```
# macOS
codex debug seatbelt [-s SANDBOX_PERMISSION]... [COMMAND]...

# Linux
codex debug landlock [-s SANDBOX_PERMISSION]... [COMMAND]...
```

You can experiment with different values of `-s` to see what permissions the `COMMAND` needs to execute successfully.

Note that the exact API for the `-s` flag is currently in flux. See https://github.com/openai/codex/issues/1248 for details.

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for Codex. Ultimately, we hope this to be a library crate that is generally useful for building other Rust/native applications that use Codex.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.
