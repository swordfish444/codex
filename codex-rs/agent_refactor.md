# Agent Runtime Refactor

## Goals
- Decouple the Codex agent loop from CLI-specific wiring so it can run as a reusable library or standalone binary.
- Preserve the current behaviour of `codex-core` (tooling, approvals, sandboxing, MCP integration) while providing a cleaner embedding surface.
- Enable specialised hosts—CLI, training harnesses, response API bridges—to share the same runtime with minimal glue code.

## Proposed Architecture
### 1. `codex-agent` crate (new)
- Owns the session runtime: `AgentRuntime`, `AgentHandle`, the states, and the task runners now under `core/src/tasks`.
- Exposes a queue-like API: `AgentHandle::submit(Op/Submission)` and `AgentHandle::next_event()` mirroring today’s behaviour.
- Re-exports protocol types from `codex-protocol` so consumers do not depend on the entire `codex-core` tree.
- Houses the agent loop (`run_task`, `run_turn`, exec/safety plumbing) together with the sandbox planner (`ExecPlan`, `PreparedExec`, etc.).

### 2. Shared configuration surface
- Introduce `AgentConfig` as the minimal runtime configuration (model, provider, approvals, sandbox defaults, cwd, user/base instructions, feature flags relevant to the loop).
- Provide `From<&Config>` for CLI compatibility; training/other hosts construct `AgentConfig` directly.
- CLI-only concerns (logging, auth prompts, workspace presets) stay inside `codex-core` and are translated before spawning the runtime.

### 3. Service abstraction layer
- Define traits that the runtime depends on instead of concrete CLI structs:
  - `CredentialsProvider` (wraps `AuthManager`).
  - `Notifier` (reuses `UserNotifier` contract).
  - `McpInterface` (start/list tools, dispatch tool calls).
  - `SandboxManager` (wraps `BackendRegistry`/`prepare_exec_invocation` wiring).
  - `RolloutSink` (write/flush rollout items; default no-op).
- Provide default implementations in `codex-core` that simply wrap the existing services (`SessionServices`).

### 4. Task subsystem consolidation
- Keep the new `SessionTask` trait and concrete tasks (`RegularTask`, `ReviewTask`, `CompactTask`) inside `codex-agent` so custom hosts can opt into additional tasks without touching CLI crates.
- Ensure task lifecycle management (`spawn_task`, `abort_all_tasks`, `ActiveTurn`) stays encapsulated in the runtime and surfaces only high-level signals (events, cancellation APIs).

### 5. Sandbox execution layer
- Move the recently created `core/src/sandbox` module into `codex-agent` (or re-export) so runtime owns exec planning.
- Runtime exposes an injectable `SandboxRuntimeConfig` (paths, seatbelt binary, stdout streaming choice) and calls into `SandboxManager` to execute plans.
- Respect existing environment variables and approval policies; no semantic changes to seatbelt handling.

### 6. Host integrations
- CLI crate: replaces direct usage of `Codex::spawn` with `AgentRuntime::spawn`, adapting CLI config/auth providers to runtime traits. Behaviour remains identical.
- Training binary (`codex-agent-bin`): thin crate that parses CLI flags (Response API URL, auth token, optional instructions) and bridges remote Ops/Events to the runtime via chosen transport (MCP channel, HTTP/WebSocket bridge).
- Additional hosts can embed the runtime by implementing the service traits and providing transport glue.

### 7. Transport adapters
- Internally keep `async_channel` for runtime queues.
- Provide helper adapters (`AgentTransport` trait) so callers can hook streams (local channel, TCP bridge, etc.) while keeping backpressure and graceful shutdown semantics consistent.

## Guidelines
- **Config boundary**: new code must depend on `AgentConfig`; only CLI/front-ends may use the broader `Config` struct. Avoid adding CLI-specific fields to the runtime config.
- **Trait-based services**: any runtime dependency that could vary across hosts (MCP, rollout persistence, sandbox execution, notifications) should be expressed as a trait with a default implementation living in `codex-core`.
- **Task authoring**: additional tasks must implement `SessionTask`; tasks are responsible for calling `run_task`/`exit_review_mode` helpers and returning final assistant output for `TaskComplete` events.
- **Sandbox safety**: all exec/patch calls must flow through `plan_exec`/`plan_apply_patch` (now under `codex-agent::sandbox`) to preserve approval semantics. Never bypass `SandboxManager`.
- **MCP usage**: runtime talks only through `McpInterface`; hosts provide concrete connectors (existing CLI manager, lightweight training stub, etc.).
- **Rollout handling**: default `RolloutSink` should no-op; hosts that require persistence (CLI, evaluation harness) supply an implementation that wraps existing recorder.
- **Transport/backpressure**: treat the runtime queue as bounded and handle cancellations; adapters must propagate `Op::Shutdown` promptly.
- **Observability**: keep tracing instrumentation intact; new modules should use existing `tracing` spans for start/end of tasks, exec calls, and MCP interactions.
- **Code quality**: write minimalist idiomatic code. Leverage the capacity of Rust 

## Current Scope Snapshot
- `codex-agent` owns the execution/runtime surface: conversation history, rollout recording, function tool plumbing, sandbox planning, command/apply_patch safety, and the new `ApprovalCoordinator` trait that abstracts user approvals. Host-agnostic helpers such as shell formatting, bash parsing, and command safety now live here.
- `codex-core` focuses on CLI integration: loading user configuration, wiring concrete services (auth, MCP, sandbox manager), translating CLI policies into runtime configs, and exposing the embedded runtime to front-ends. It re-exports runtime modules needed by existing callers but should avoid hosting new agent logic.
- Session bootstrap now flows through a host-provided `prepare_session_bootstrap` helper: the CLI constructs rollout/MCP/sandbox services, builds the new `codex_agent::SessionServices` + `SessionState`, pre-builds the initial `TurnContext` (model client + tool config), and hands them to `Session::new` instead of constructing them inline.


## Implementation Plan
1. **Baseline & documentation**
   - Capture current interfaces (`Codex`, `Session`, `SessionTask`) and update developer docs to reference this refactor plan.
   - Add smoke tests covering multi-task scenarios (regular + review + compact) to guard against regressions during extraction.

2. **Introduce `AgentConfig`**
   - Define struct + conversion helpers inside `codex-core`.
   - Refactor internal `Session::new` / `TurnContext` builders to accept `AgentConfig` without changing external behaviour.

3. **Service trait extraction**
   - Carve out trait definitions (`CredentialsProvider`, `McpInterface`, `SandboxManager`, `RolloutSink`, `Notifier`).
   - Provide adapters backed by existing `SessionServices`.
   - Update `Session` and helper modules to depend on traits rather than concrete structs.

4. **Create `codex-agent` crate**
   - Scaffold crate, move runtime modules (`codex.rs`, `state`, `tasks`, `sandbox`) while keeping module paths stable via `pub use` re-exports.
   - Resolve module imports to reference trait abstractions / helper crates (e.g., `codex_protocol`, `codex-apply-patch`).
   - Ensure crate exposes `AgentRuntime`, `AgentHandle`, and service traits.

5. **Adapt `codex-core`**
   - Replace `Codex::spawn` with thin wrapper that constructs `AgentConfig`, runtime service adapters, and delegates to `codex-agent`.
   - Update public API to re-export runtime types if downstream crates expect them.
   - Confirm unit tests continue to pass.

6. **Update front-ends**
   - CLI crate: switch to new runtime API; verify login/auth flows, approvals, and sandbox invocations.
   - Other binaries (`chatgpt`, etc.) migrate similarly, adjusting imports/config conversions.

7. **Add training binary**
   - Implement new `codex-agent-bin` crate providing CLI for Response API URL + auth.
   - Reuse existing MCP client logic where possible; otherwise, provide minimal HTTP bridge translating Ops/Events.
   - Add integration tests using mocked Response API.

8. **Refine transport adapters**
   - Add optional helper module offering channel/TCP/WebSocket adapters along with graceful shutdown behaviour.
   - Document how hosts select or implement transports.

9. **Finalize rollout persistence strategy**
   - Implement `RolloutSink` adapters (file-based, in-memory, disabled).
   - Ensure CLI wires existing recorder; training binary can opt in/out via flags.

10. **Docs & polish**
    - Update repository documentation (`README`, architecture docs) to reference the new crates and APIs.
    - Record migration notes for downstream consumers.
    - Run `just fmt`, scoped `just fix -p`, and targeted tests for touched crates before merging.

11. **Validation**
    - Execute `cargo test -p codex-agent`, `cargo test -p codex-core`, and full suite (`cargo test --all-features`) once shared crates change.
    - Perform manual verification: CLI session, review task, training binary against mock Response API, ensuring approvals and sandboxing behave identically.
