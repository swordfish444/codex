**DOs**
- Clone `Arc`s consistently with `.clone()`: prefer `session.clone()` over `Arc::clone(&session)`.
```rust
// Preferred
tokio::spawn(submission_loop(session.clone(), turn_context, config, rx_sub));
```
- Reuse shared spawn flow: factor event validation/registration into a helper (e.g., `finalize_spawn`) and call it from both regular and forked spawns.
```rust
let CodexSpawnOk { codex, session_id } =
    Codex::spawn(config, auth_manager, Some(truncated_history)).await?;
self.finalize_spawn(codex, session_id).await
```
- Design clear, intention-revealing APIs: put the main subject first, modifiers second, plumbing last; name intermediates clearly (e.g., `truncated_history`).
```rust
pub async fn fork_conversation(
    &self,
    conversation_history: Vec<ResponseItem>,
    num_messages_to_drop: usize,
    config: Config,
) -> CodexResult<NewConversation> {
    let truncated_history =
        truncate_after_dropping_last_messages(conversation_history, num_messages_to_drop);
    // spawn with truncated_history...
}
```
- Use `Option::or_else` for clean fallbacks when choosing initial history.
```rust
let restored_items: Option<Vec<ResponseItem>> = initial_history.or_else(|| {
    maybe_saved.and_then(|saved|
        if saved.items.is_empty() { None } else { Some(saved.items) })
});
```
- Keep protocol cohesive and documented: colocate related ops and add docstrings; use consistent payload names.
```rust
// Near GetHistoryEntryRequest
/// Request the full in-memory conversation transcript for the current session.
/// Reply is delivered via `EventMsg::ConversationHistory`.
Op::GetHistory,

// Event and payload
pub enum EventMsg {
    // ...
    ConversationHistory(ConversationHistoryResponseEvent),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationHistoryResponseEvent {
    pub conversation_id: Uuid,
    pub entries: Vec<ResponseItem>,
}
```
- Prefer clear control flow and tests for history truncation helpers.
```rust
fn truncate_after_dropping_last_messages(items: Vec<ResponseItem>, n: usize) -> Vec<ResponseItem> {
    if n == 0 || items.is_empty() {
        return items;
    }
    // ... compute cut_index and count ...
    if count < n {
        Vec::new()
    } else {
        items.into_iter().take(cut_index).collect()
    }
}

#[test]
fn drops_from_last_user_only() {
    let items = vec![user_msg("u1"), assistant_msg("a1"), user_msg("u2"), assistant_msg("a2")];
    let out = truncate_after_dropping_last_messages(items.clone(), 1);
    assert_eq!(out, vec![items[0].clone(), items[1].clone()]);
}
```

**DON'Ts**
- Don’t expand `Codex`’s responsibilities by storing a `Session` inside it.
```rust
// Avoid adding fields like this to Codex:
struct Codex {
    next_id: AtomicU64,
    tx_sub: Sender<Submission>,
    rx_event: Receiver<Event>,
    // session: Arc<Session>, // ❌ Avoid
}
```
- Don’t use `Arc::clone(&x)` in this codebase where `.clone()` is the project norm.
```rust
// Avoid
tokio::spawn(submission_loop(Arc::clone(&session), turn_context, config, rx_sub));
```
- Don’t duplicate “first event must be SessionConfigured” validation across spawn paths—centralize it.
```rust
// Anti-pattern: redoing validation inline
let event = codex.next_event().await?;
// ... custom matching logic duplicated here ...
// ✅ Prefer: self.finalize_spawn(codex, session_id).await
```
- Don’t use vague parameter ordering or names in APIs like `fork_conversation`.
```rust
// Avoid
pub async fn fork_conversation(&self, drop_last_messages: usize, config: Config, items: Vec<ResponseItem>);
```
- Don’t introduce inconsistent or unclear protocol naming, or undocumented ops.
```rust
// Avoid ambiguous/unused name
pub struct ConversationHistoryEvent; // ❌ Use ConversationHistoryResponseEvent

// Avoid undocumented, misplaced op
Op::GetHistory, // ❌ without docstring or near related ops
```