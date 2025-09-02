**DOs**
- **Keep PRs Focused**: Limit changes to moving `models.rs` and the required import updates; defer unrelated protocol/API additions to a separate PR.
```rust
// Good: only path + import updates

// core/src/lib.rs
- mod models;
+ // models moved to protocol

// any module using models
- use crate::models::ResponseItem;
+ use codex_protocol::models::ResponseItem;
```

- **Update Imports Consistently**: Replace `crate::models::*` with `codex_protocol::models::*` across the codebase.
```rust
- use crate::models::{ContentItem, ResponseItem, ReasoningItemContent};
+ use codex_protocol::models::{ContentItem, ResponseItem, ReasoningItemContent};
```

- **Use Fully Qualified Paths In Matches When Helpful**: Avoid ambiguity after the move, especially in pattern matches.
```rust
let is_assistant_delta = matches!(
    &item,
    codex_protocol::models::ResponseItem::Message { role, .. } if role == "assistant"
);
```

- **Justify Protocol Dependencies**: Add deps (e.g., `base64`, `mime_guess`, `tracing`) to `protocol` only if `models.rs` truly needs them for serialization or type helpers; note why in the PR.
```toml
# codex-rs/protocol/Cargo.toml
[dependencies]
base64 = "0.22.1"      # encoding attachments in models
mime_guess = "2.0.5"   # inferring content-types for file items
tracing = "0.1.41"     # protocol-level diagnostics (if unavoidable)
```
```rust
// Example: encoding a payload inside protocol models
let encoded = base64::encode(&bytes);
```

- **Preserve Behavior While Relocating**: Moves should not change runtime logic or filtering; keep conversions and aggregations identical.
```rust
// Keep existing content mapping behavior intact when moving code.
content: items
    .iter()
    .filter_map(|i| match i {
        InputItem::Text { text } => Some(ContentItem::InputText { text: text.clone() }),
        // keep prior arms and catch-alls as-is to avoid behavior drift
        _ => None,
    })
    .collect(),
```

- **Validate With Targeted Tests First**: Run protocol and core tests after changes; then run the full suite if common types changed.
```bash
# Format + lint (from codex-rs/)
just fmt
just fix -p codex-protocol
cargo test -p codex-protocol
cargo test -p codex-core
cargo test --all-features
```

- **Be Explicit When Constructing Moved Types**: Build items using the new module path to make intent obvious.
```rust
let aggregated = codex_protocol::models::ResponseItem::Message {
    id: None,
    role: "assistant".to_string(),
    content: vec![codex_protocol::models::ContentItem::OutputText {
        text: buffer,
    }],
};
```

**DON’Ts**
- **Don’t Introduce Unrelated Protocol Ops/Events**: Avoid adding new variants like `Op::GetHistory` or `EventMsg::ConversationHistory` in a “move-only” PR.
```rust
// Bad: out-of-scope for a file move
pub enum Op {
    Compact,
    Shutdown,
    GetHistory, // <-- why in this PR?
}

pub enum EventMsg {
    ShutdownComplete,
    ConversationHistory(ConversationHistoryEvent), // <-- unrelated addition
}
```

- **Don’t Add New API Variants Without Rationale**: Adding `ResponseItem::CustomToolCall` expands the wire protocol—separate PR with justification and downstream updates.
```rust
// Bad: new wire variant slipped into a refactor
pub enum ResponseItem {
    Message { /* ... */ },
    ToolCall { /* ... */ },
    FunctionCallOutput { /* ... */ },
    CustomToolCall { /* ... */ }, // <-- why now?
}
```

- **Don’t Expand Protocol Responsibilities Needlessly**: Keep business logic out of `protocol`; it should define types and minimal helpers only.
```rust
// Bad: complex aggregation/business logic in protocol
impl ResponseItem {
    pub fn aggregate_streaming_reasoning(...) -> Self { /* non-protocol logic */ }
}
```

- **Don’t Create Dependency Drag**: If a helper can live in `core`, don’t add a dependency to `protocol` just to support it.
```toml
# Bad: protocol now depends on a heavy crate used only by core features
some-heavy-crate = "1"
```

- **Don’t Change Formatting/Behavior While Moving**: Avoid incidental logic edits (e.g., removing a match arm) that alter filtering or emitted items.
```rust
// Bad: removing a catch-all subtly changes behavior
- _ => None,
+ // (removed) now non-matching items are silently dropped differently
```

- **Don’t Leave Type Names Ambiguous**: After the move, don’t rely on old paths or shadowed names.
```rust
// Bad
use crate::models::*; // no longer valid, confusing after the move

// Good
use codex_protocol::models::{ResponseItem, ContentItem};
```