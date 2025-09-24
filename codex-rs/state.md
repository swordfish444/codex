# TurnState Refactor Plan

## Motivation
- The current `State` inside `core/src/codex.rs` mixes long-lived session data (e.g. history, approved commands) with per-turn information (`pending_input`, transient readiness flags).
- Readiness is delivered over a side channel and threaded through many call sites as an `Option<Arc<ReadinessFlag>>`, making it hard to reason about when the flag is consumed, reused, or replaced.
- Follow-up user messages while a turn is streaming are pushed into a session-level vector; the lifecycle for those items is implicit inside `run_task`.
- Introducing an explicit `TurnState` object lets us capture everything that belongs to one user turn, ensures it is dropped when the turn finishes, and gives us a single place to own the readiness flag.

## Proposed data structures

### `TurnState`
```rust
pub(crate) struct TurnState {
    /// Submission id that started this turn.
    pub sub_id: String,
    /// Per-turn context (model overrides, cwd, schema…).
    pub turn_context: Arc<TurnContext>,
    /// The initial user input bundled for the model.
    pub initial_input: ResponseInputItem,
    /// Mailbox for follow-up user inputs and readiness handoffs.
    mailbox: Mutex<TurnMailbox>,
    /// Tracks the latest agent output / review artefacts for completion events.
    last_agent_message: Mutex<Option<String>>,
    /// When in review mode we keep an isolated history to feed the child model.
    review_thread_history: Mutex<Vec<ResponseItem>>,
    /// Tracks the diff across the whole turn so we can emit `TurnDiff` events once.
    diff_tracker: Mutex<TurnDiffTracker>,
    /// Whether we already tried auto-compaction in this turn.
    auto_compact_recently_attempted: AtomicBool,
}
```

`TurnMailbox` is a helper that keeps the queue of pending turn inputs and the most recent readiness flag:
```rust
struct TurnMailbox {
    latest_readiness: Option<Arc<ReadinessFlag>>,
    pending: VecDeque<PendingTurnInput>,
}
```

`PendingTurnInput` keeps the shape it already has today (`ResponseInputItem` plus the readiness flag that was active when it was enqueued).

### Handle vs. runtime
- `TurnState` is reference-counted (`Arc<TurnState>`) so both the session (for injection) and the running task can access it.
- Runtime-only helpers (prompt building, retry counters) remain inside `run_task`; they borrow data from `TurnState` instead of keeping their own copies.

## Lifecycle management
1. **Creation** – When `submission_loop` receives `Op::UserInput` or `Op::UserTurn` and there is no active task, it constructs a `TurnState`:
   - Build/resolve the `TurnContext` (either reuse the persistent one or apply the per-turn overrides).
   - Collect the initial readiness flag by peeking at the readiness receiver. Instead of losing it on `try_recv` failure we push the flag into a queue owned by the session; `TurnState::new` pops from that queue. If nothing is available we store `None` and the flag defaults to ready semantics.
   - Convert the submitted items into a `ResponseInputItem` and seed the mailbox with that entry. The same `TurnState::enqueue_initial_input` helper is used for review threads so every task goes through the same path.
   - Wrap the whole struct in an `Arc` and pass it to `AgentTask::spawn` (or `.review`).

2. **Session bookkeeping** – `State` gains two fields:
   ```rust
   current_task: Option<AgentTask>,
   current_turn: Option<Arc<TurnState>>,
   ```
   `Session::set_task` stores both, aborting the previous task if needed. `Session::remove_task` clears `current_turn` in addition to `current_task`.

3. **Injecting more user input** – `Session::inject_input` becomes a thin wrapper:
   - Grab the session mutex.
   - If there is an active `TurnState`, call `turn_state.enqueue_user_input(items, readiness)` and return `Ok(())`.
   - If not, return `Err((items, readiness))` so the submission loop knows it needs to start a fresh turn (same behaviour as today).
   The enqueue helper converts the new items into a `PendingTurnInput`, pushes it into the mailbox, and updates `latest_readiness` when a flag accompanies the message.

4. **Turn execution** – `run_task` now receives `Arc<TurnState>` instead of a raw `Vec<InputItem>` / readiness pair. It:
   - Grabs the initial input via `turn_state.take_initial_input()` to seed history and the review mailbox.
   - On each iteration, calls `turn_state.drain_mailbox()` which returns `(Vec<ResponseItem>, Option<Arc<ReadinessFlag>>)` so the loop no longer needs to manipulate the readiness flag manually. `TurnMailbox` ensures we always hand out the most recent readiness flag (the newest non-`None` entry wins).
   - Accesses the diff tracker, review history, and auto-compaction flag through the `TurnState` rather than local variables. This keeps the single source of truth tied to the turn’s lifetime and makes debugging easier.
   - Writes the last assistant message into `turn_state` before signalling `TaskComplete` so listeners can retrieve it even if the task is aborted elsewhere.

5. **Completion** – When the loop finishes (success, interruption, or error) we drop `Arc<TurnState>` by clearing `current_turn`. All readiness waiters associated with the turn naturally drop because the only owner lives on the turn state.

## Readiness handling
- `TurnReadinessBridge` in the TUI continues to send `Arc<ReadinessFlag>` values over the readiness channel; the session stores them in a short queue (`VecDeque<Arc<ReadinessFlag>>`) protected by the same mutex that guards `State`.
- `TurnState::new` pops the next flag when constructing the mailbox. If the queue is empty we log (with rate limiting) and store `None` so the turn stays unblocked.
- `TurnState::enqueue_user_input` accepts an optional flag. When present we update `latest_readiness` before pushing the input so subsequent `drain_mailbox` calls hand the new flag to `run_turn`.
- `run_turn` and `handle_response_item` only see `turn_state.current_readiness()`, eliminating the need for an ad-hoc `current_turn_readiness` variable scattered through the loop.
- Because the readiness flag lives on the `TurnState`, tool handlers that are spawned outside the loop (e.g. background exec streams) can clone the flag from the turn state if they need to delay until the user confirms.

## Changes to submission loop
- Replace the existing `turn_readiness_rx.try_recv()` calls with a helper on the session such as `Session::next_turn_readiness()` that returns the oldest queued flag (or `None`). `TurnState::new` receives that value and stores it in its mailbox.
- The submission loop no longer passes readiness into `AgentTask::spawn`; instead it constructs the `TurnState` (with readiness embedded) and hands the state to the task constructor.
- For review turns and compaction tasks, we construct a `TurnState` with `None` readiness. The helper works for both flows so we can remove the separate code paths that bypass readiness today.

## Implementation plan
1. Introduce the `turn_state` module with `TurnState`, `TurnMailbox`, and helpers to enqueue / drain inputs and expose readiness.
2. Extend `State` with `current_turn` and a `VecDeque<Arc<ReadinessFlag>>` used to store unread readiness flags pushed by the UI.
3. Update the readiness sender plumbing so `Codex::turn_readiness_sender()` pushes into that queue; remove the direct `try_recv` usage.
4. Refactor `AgentTask::spawn` / `run_task` to accept `Arc<TurnState>` and use the new helper methods for initial input, pending input, diff tracking, and readiness.
5. Simplify `Session::inject_input` to route through the active `TurnState` instead of manipulating `state.pending_input` directly. Drop the `PendingTurnInput` vector from `State` once all call sites are migrated.
6. Move per-turn temporaries (`last_agent_message`, review mailbox, diff tracker, auto-compact flag) into `TurnState`; this lets us delete the bespoke locals in `run_task` and make the turn lifecycle self-contained.
7. After the refactor, audit call sites to ensure readiness is consistently fetched from the turn state, delete the now-unused `turn_readiness` parameters, and clean up warnings.

## Follow-up considerations
- With `TurnState` owning the readiness flag we can extend it later to expose richer readiness semantics (e.g. multiple tokens, logging) without touching the submission loop again.
- This refactor lays the groundwork for queuing multiple `TurnState`s if we later want to support full multiturn buffering instead of mutating the live turn.
- Once `TurnState` is in place, the session-level mutex guards much less data, which could be split further if concurrency becomes a bottleneck.
