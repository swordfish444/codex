# Tool Handling Refactor Plan

## Current Pain Points
- Tool specs and dispatch live in multiple files (`openai_tools.rs`, `codex.rs`, `shell.rs`, `apply_patch.rs`, `exec_command`, `plan_tool`), so every tool is stitched together by hand with duplicated string matches.
- `codex.rs` mixes concerns: argument parsing, apply-patch verification, approval policy enforcement, telemetry, and response construction for Function/Custom/LocalShell calls are all intertwined.
- Adding or adjusting a tool requires touching 5–7 sites and remembering subtle invariants (e.g., apply_patch needs extra verification, view_image requires path resolution), which increases review overhead and bug risk.
- Spec configuration (`ToolsConfig`, `get_openai_tools`) is disconnected from runtime dispatch, so there is no single registry that knows which tools are active or how to invoke them.

## Goals
- Centralize tool definitions, specs, and dispatching inside a dedicated `tools` module with clearly defined traits and types.
- Unify handling for function calls, custom calls, and local shell calls, sharing approval checks, apply_patch guards, and telemetry.
- Slim down `codex.rs` so it only converts `ResponseItem` into a `ToolCall` and delegates the rest.
- Make new tool onboarding require (at most) one handler + registration entry + optional tests.

## Core Traits & Types
- ```rust
  pub trait ToolHandler: Send + Sync {
      fn spec(&self) -> Option<&ToolSpec>;
      fn kind(&self) -> ToolKind;
      async fn handle(&self, invocation: ToolInvocation<'_>) -> Result<ToolOutput, FunctionCallError>;
  }
  ```
  - `spec`: supplies the OpenAI tool definition for function/custom tools (returns `None` for implicit tools like local shell).
  - `kind`: declares whether the handler services `ToolCall::Function`, `ToolCall::Custom`, `ToolCall::LocalShell`, etc., for dispatch filtering.
  - `handle`: runs the tool logic, returning structured output + optional streaming side effects.
- ```rust
  pub struct ToolInvocation<'a> {
      pub session: &'a Session,
      pub turn: &'a TurnContext,
      pub tracker: &'a mut TurnDiffTracker,
      pub sub_id: &'a str,
      pub call_id: &'a str,
      pub payload: ToolPayload<'a>,
  }
  ```
  - `payload` captures parsed arguments (`serde_json::Value` for functions/custom, `ShellToolCallParams` for local shell, etc.).
- ```rust
  pub enum ToolOutput {
      Function(FunctionCallOutputPayload),
      Custom(String),
  }
  ```
  - Helper constructors convert to `ResponseInputItem` in one place.
- ```rust
  pub struct ToolRegistry {
      handlers: HashMap<ToolName, Arc<dyn ToolHandler>>,
  }
  ```
  - Provides `register_handler`, `iter_specs()` for prompt assembly, and `dispatch(call: ToolCall) -> Result<ResponseInputItem, FunctionCallError>`.

## Proposed Refactor
1. **Scaffold `core/src/tools` Module**
   - `mod.rs` re-exports `context`, `registry`, `router`, `spec`, and `handlers::*`.
   - `context.rs` defines `ToolInvocation`, `ToolPayload`, `ToolOutput`, helper constructors for `ResponseInputItem`.
   - `registry.rs` implements `ToolRegistry` + `ToolKind` enum and houses the instrumentation wrapper (`otel.log_tool_result`, approval policy checks).
   - `router.rs` provides `ToolCall` enum (variants: `Function`, `Custom`, `LocalShell`, `UnifiedExec`, `Mcp`), plus `Router::from_turn_context(&TurnContext)` to build the registry + spec list together.

2. **Move Spec Logic Into `tools::spec`**
   - Relocate `ToolsConfig`, `ToolsConfigParams`, `get_openai_tools`, `ConfigShellToolType`, and tool builders (shell/unified exec/apply_patch/etc.) into `spec.rs`.
   - Add `ToolSpec` struct mirroring current `OpenAiTool`, keeping existing tests but updated paths.
   - Expose `pub fn build_specs(config: &ToolsConfig, mcp: Option<HashMap<..>>) -> (Vec<ToolSpec>, ToolRegistryBuilder)` to keep spec generation and handler registration in sync.

3. **Implement Concrete Handlers**
   - Split existing logic into focused files:
     1. `handlers/shell.rs`: wraps `handle_container_exec_with_params`, approval policy enforcement, apply_patch verification, and streaming support.
     2. `handlers/apply_patch.rs`: retains apply_patch-specific parsing when invoked as a function/custom tool.
     3. `handlers/unified_exec.rs`: hosts `handle_unified_exec_tool_call` logic.
     4. `handlers/plan.rs`: wraps `handle_update_plan`.
     5. `handlers/view_image.rs`: handles local path resolution + `inject_input` call.
     6. `handlers/exec_stream.rs`: handles `exec_command` and `write_stdin` (for streamable exec shell variant).
     7. `handlers/mcp.rs`: optional adapter that calls through to `handle_mcp_tool_call` while emitting consistent telemetry.
   - Each handler implements `ToolHandler` with explicit payload parsing (`serde_json::from_value` helpers) and reuses shared utilities (`create_env`, `maybe_parse_apply_patch_verified`).

4. **Registry Wiring**
   - Add `ToolRegistryBuilder` helper collecting `(ToolName, Arc<dyn ToolHandler>)` pairs.
   - Builder methods (e.g., `with_shell_handler`, `with_apply_patch_handler`) are invoked from `spec::build_specs` based on `ToolsConfig` decisions so that enabling a spec automatically registers the handler.
   - Integrate MCP tools by registering a generic `McpHandler` per tool when MCP discovery runs.

5. **Update `codex.rs`**
   - Replace `handle_function_call`, `handle_custom_tool_call`, `to_exec_params`, and `handle_container_exec_with_params` with:
     - `let router = tools::router::Router::new(sess, turn_context, turn_diff_tracker);`
     - Convert `ResponseItem` into `ToolCall` and call `router.dispatch(...)`.
   - Remove duplicate instrumentation; `ToolRegistry` handles `log_tool_result` and error wrapping.
   - Keep only MCP discovery + fallback logic, delegating to `ToolRegistry` for execution.

6. **Tests & Validation**
   - Port existing unit tests in `openai_tools.rs` to `tools::spec` and add new tests for `ToolRegistry` covering success, error, and approval-policy rejection paths.
   - Add smoke tests for converting `ResponseItem` to `ToolCall` in `tools::router`.
   - Ensure apply_patch verification works both for function and local shell flows via dedicated tests (mock `apply_patch::apply_patch`).

## Testing & Rollout
- Run `cargo test -p codex-core tools::spec` (once added) plus focused handler tests.
- Execute existing integration suites that cover exec/apply_patch/unified_exec flows.
- No behavior change expected, but snapshot suites (esp. TUI if any output differs) should be re-run as a sanity check.

## Follow-ups
- With registry centralization, expose structured telemetry (counts, durations) and hook into metrics.
- Evaluate deletion of legacy local shell pathway once unified_exec proves stable.
