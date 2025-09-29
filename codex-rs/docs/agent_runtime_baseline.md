# Agent Runtime Baseline

Codex currently exposes its agent runtime from the `codex-core` crate. The runtime is organised around three cooperating interfaces located in `core/src`:

- `Codex` (`core/src/codex.rs`) is the public façade that hosts use. It owns the submission/event queues and spawns the asynchronous runtime.
- `Session` (`core/src/codex.rs`) encapsulates conversation-scoped state and orchestrates task lifecycles once a session has been configured.
- `SessionTask` (`core/src/tasks/mod.rs`) is the trait implemented by the concrete task runners (`RegularTask`, `ReviewTask`, `CompactTask`).

The refactor tracked in `../agent_refactor.md` will extract these responsibilities into a dedicated `codex-agent` crate; this document captures the current layout before the extraction begins.

## `Codex`

`Codex` is the high-level queue API that front-ends interact with:

- `spawn` initialises the runtime with a `Config`, `AuthManager`, and `InitialHistory` and returns a `CodexSpawnOk` containing the queue endpoints and generated `ConversationId`.
- `submit` wraps an `Op` in a `Submission`, generates a unique id, and pushes it onto the bounded submission channel (`SUBMISSION_CHANNEL_CAPACITY = 64`).
- `next_event` pulls the next `Event` from the unbounded event receiver, propagating `CodexErr::InternalAgentDied` if the channel is closed.

Upon spawning, `Codex` constructs a `ConfigureSession` payload that gathers CLI-derived configuration (model details, approvals, sandbox policy, notify hooks, `cwd`) and delegates to `Session::new`. It also seeds the `TurnContext`, `SessionServices`, and kicks off the background `submission_loop` task that drives the agent.

## `Session`

`Session` bundles the pieces required to handle a configured conversation:

- Identifiers: `conversation_id` (originating from `InitialHistory`) and an internal `next_internal_sub_id` counter for action-specific ids.
- Communication: the `tx_event` sender used to emit `Event` messages back to the host.
- State holders: a `Mutex<SessionState>` for persistent session data and a `Mutex<Option<ActiveTurn>>` tracking the currently running tasks.
- Services: `SessionServices` packages dependencies that are currently hard-wired to CLI types—`ExecSessionManager`, `UnifiedExecSessionManager`, `RolloutRecorder`, `McpConnectionManager`, etc.

`Session::new` performs the upfront wiring: it initialises rollout recording, MCP connections, default shell discovery, and history metadata in parallel; constructs the `SessionServices`; emits the initial `SessionConfigured` event; and records any startup warnings so they can be surfaced after configuration.

Operationally, `Session` is responsible for:

- Translating incoming `Submission`s into task invocations via `run_task`, `run_turn`, and helper functions in `core/src/codex.rs` and `core/src/tasks`.
- Managing approvals and sandbox execution by calling into `sandbox::plan_*` helpers and dispatching events (`ExecApprovalRequestEvent`, `ApplyPatchApprovalRequestEvent`, etc.).
- Recording rollout items and forwarding MCP tool call updates.
- Tracking and cancelling running work when new input arrives or when approvals are rejected.

## `SessionTask`

Tasks implement the asynchronous work units that execute within a session. The trait lives in `core/src/tasks/mod.rs`:

```rust
#[async_trait]
pub(crate) trait SessionTask: Send + Sync + 'static {
    fn kind(&self) -> TaskKind;
    async fn run(
        self: Arc<Self>,
        session: Arc<SessionTaskContext>,
        ctx: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Option<String>;

    async fn abort(&self, session: Arc<SessionTaskContext>, sub_id: &str) { ... }
}
```

`SessionTaskContext` is a thin wrapper that hands tasks a clone of the `Session`, giving them access to helpers such as `send_event`, `plan_exec`, and `run_with_plan`. `Session::spawn_task` ensures only one task runs at a time by:

1. Calling `abort_all_tasks` to cancel the current `ActiveTurn`.
2. Wrapping the concrete task in `Arc<dyn SessionTask>`.
3. Spawning a Tokio task that awaits `run` and then reports completion through `Session::on_task_finished`.

`RunningTask`, `ActiveTurn`, and `TurnAbortReason` (from `core/src/state`) coordinate cancellation semantics and surface `TurnAborted`/`TaskComplete` events consistently.

Today the concrete implementations are:

- `RegularTask` (`core/src/tasks/regular.rs`) for the standard Codex workflow.
- `ReviewTask` (`core/src/tasks/review.rs`) used during review mode.
- `CompactTask` (`core/src/tasks/compact.rs`) which emits summarised history.

Each uses shared utilities in `core/src/codex.rs` (e.g., `run_task`, `exit_review_mode`, sandbox planners) and relies on the CLI-flavoured services packaged in `SessionServices`.

## Next Steps

With this baseline documented, the next implementation steps are described in `../agent_refactor.md`. As we move work into the new `codex-agent` crate we should revisit this document to ensure the captured interfaces stay accurate and to outline any newly introduced abstractions (`AgentRuntime`, `AgentConfig`, service traits, etc.).
