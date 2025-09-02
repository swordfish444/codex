**DOs**
- Bold the keyword: Correlate deltas with item IDs. Attach `item_id` to delta events so clients can match deltas to the final item.
```rust
match event_type.as_str() {
    "response.output_text.delta" => {
        if let (Some(delta), Some(item_id)) = (event.delta, event.item_id) {
            let ev = ResponseEvent::OutputTextDelta { delta, item_id };
            tx_event.send(Ok(ev)).await.ok();
        }
    }
    "response.reasoning_summary_text.delta" => {
        if let (Some(delta), Some(item_id)) = (event.delta, event.item_id) {
            tx_event.send(Ok(ResponseEvent::ReasoningSummaryDelta { delta, item_id }))
                .await.ok();
        }
    }
    _ => {}
}
```

- Bold the keyword: Prefer envelope `Event.id` when it’s the same concept. If the envelope already carries the identifier you need, don’t duplicate it inside the payload.
```rust
let event = Event {
    // Use the item identifier directly on the envelope when appropriate.
    id: item_id.clone(),
    msg: EventMsg::AgentMessage(AgentMessageEvent { message: text }),
};
```

- Bold the keyword: Use `..` or `_` to ignore unused fields. Keep matches resilient to schema growth.
```rust
match item {
    ResponseItem::Message { role, content, .. } => { /* ... */ }
    ResponseItem::Reasoning { id, summary } => { /* ... */ }
}

match item {
    ResponseItem::Message { id: _, role, content } => { /* ... */ }
    _ => {}
}
```

- Bold the keyword: Set `id: None` for input items. Make it explicit that request-side items don’t carry server-assigned IDs.
```rust
ResponseItem::Message {
    id: None,
    role: "assistant".to_string(),
    content: vec![ContentItem::OutputText { text }],
}
```

- Bold the keyword: Update all consumers when shapes change. Thread new fields through exec, TUI, MCP, etc., even if you ignore them.
```rust
// exec
EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta, item_id: _ }) => { /* ... */ }

// tui
EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta, .. }) => { /* ... */ }
```

- Bold the keyword: Log with context at `trace`. Emit low-noise logs to help diagnose streaming pipelines.
```rust
trace!(?event, "output_item.done");
trace!(?event, "sending event as notification");
```

**DON’Ts**
- Bold the keyword: Don’t duplicate identifiers without need. Avoid carrying the same ID on both the envelope and the payload.
```rust
// Avoid if Event.id already identifies the same thing:
Event {
    id: item_id.clone(),
    msg: EventMsg::AgentMessage(AgentMessageEvent {
        // Redundant if equal to Event.id:
        id: Some(item_id.clone()),
        message: text,
    }),
};
```

- Bold the keyword: Don’t drop `item_id` from deltas. Deltas must be correlatable to their final `output_item.done`.
```rust
// ❌ Wrong
ResponseEvent::OutputTextDelta(delta);

// ✅ Right
ResponseEvent::OutputTextDelta { delta, item_id };
```

- Bold the keyword: Don’t match exhaustively on all fields. Exact-field matches cause churn when structs evolve.
```rust
// ❌ Fragile
if let ResponseItem::Message { role, content } = item { /* ... */ }

// ✅ Stable
if let ResponseItem::Message { role, content, .. } = item { /* ... */ }
```

- Bold the keyword: Don’t require IDs on request inputs. Server assigns IDs; input-side IDs should remain absent.
```rust
// ❌ Don’t invent client-side IDs for requests
ResponseItem::Message { id: Some("local-123".into()), role, content };

// ✅ Keep it empty
ResponseItem::Message { id: None, role, content };
```

- Bold the keyword: Don’t conflate different ID meanings. If `Event.id` is a subscription/event stream ID, don’t reuse it for item identity.
```rust
// Keep separate when semantics differ:
let event = Event {
    id: sub_id.clone(), // stream/subscription
    msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
        delta,
        item_id, // response item correlation
    }),
};
```