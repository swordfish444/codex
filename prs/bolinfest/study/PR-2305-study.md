**DOs**
- Parallelize independent setup: Use `tokio::join!` to run RolloutRecorder init/resume, MCP startup, default shell detection, and history metadata loading concurrently.
```rust
let rollout_fut = async { /* resume or new RolloutRecorder */ };
let mcp_fut = McpConnectionManager::new(config.mcp_servers.clone());
let default_shell_fut = shell::default_user_shell();
let history_meta_fut = crate::message_history::history_metadata(&config);

let (rollout_res, mcp_res, default_shell, (history_log_id, history_entry_count)) =
    tokio::join!(rollout_fut, mcp_fut, default_shell_fut, history_meta_fut);
```

- Let RolloutRecorder determine `session_id`: Build the model client only after the final `session_id` is known.
```rust
struct RolloutResult {
    session_id: Uuid,
    rollout_recorder: Option<RolloutRecorder>,
    restored_items: Option<Vec<ResponseItem>>,
}

let RolloutResult { session_id, rollout_recorder, restored_items } = rollout_result;

let client = ModelClient::new(
    config.clone(),
    auth.clone(),
    provider.clone(),
    model_reasoning_effort,
    model_reasoning_summary,
    session_id,
);
```

- Fail hard on explicit resume errors; degrade gracefully otherwise: If resuming fails with a provided path, return an error; if creating a new recorder fails, warn and enqueue a user-facing error event.
```rust
match rollout_res {
    Ok(tuple) => { /* use recorder + saved items */ }
    Err(e) => {
        if let Some(path) = resume_path.as_ref() {
            return Err(anyhow::anyhow!("failed to resume rollout from {path:?}: {e}"));
        }
        let message = format!("failed to initialize rollout recorder: {e}");
        post_session_configured_error_events.push(Event {
            id: INITIAL_SUBMIT_ID.to_owned(),
            msg: EventMsg::Error(ErrorEvent { message: message.clone() }),
        });
        warn!("{message}");
        // proceed with a fresh session_id and no recorder
    }
}
```

- Emit SessionConfigured before error events: Acknowledge the session first, then surface any startup failures.
```rust
let events = std::iter::once(Event {
    id: INITIAL_SUBMIT_ID.to_owned(),
    msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
        session_id,
        history_log_id,
        history_entry_count,
        default_shell,
        writable_roots,
    }),
})
.chain(post_session_configured_error_events.into_iter());

tx_event.send_many(events).await?;
```

- Batch conversation item recording: Record user instructions and environment context in one call.
```rust
let mut items = Vec::with_capacity(2);
if let Some(ui) = sess.user_instructions.as_deref() {
    items.push(Prompt::format_user_instructions_message(ui));
}
items.push(ResponseItem::from(EnvironmentContext::new(
    sess.get_cwd().to_path_buf(),
    sess.get_approval_policy(),
    sess.sandbox_policy.clone(),
)));
sess.record_conversation_items(&items).await;
```

- Handle MCP startup results and per-client failures: Default to an empty manager on errors and surface failures to users.
```rust
let (mcp_connection_manager, failed_clients) = match mcp_res {
    Ok((mgr, failures)) => (mgr, failures),
    Err(e) => {
        let message = format!("Failed to create MCP connection manager: {e:#}");
        error!("{message}");
        post_session_configured_error_events.push(Event {
            id: INITIAL_SUBMIT_ID.to_owned(),
            msg: EventMsg::Error(ErrorEvent { message }),
        });
        (McpConnectionManager::default(), Default::default())
    }
};

// For each failed client:
for (name, why) in failed_clients {
    let message = format!("MCP client '{name}' failed to start: {why:#}");
    post_session_configured_error_events.push(Event {
        id: INITIAL_SUBMIT_ID.to_owned(),
        msg: EventMsg::Error(ErrorEvent { message }),
    });
}
```

- Understand `unwrap()` on `Mutex<Option<T>>`: The `unwrap()` is for the lock; use `as_ref()`/`cloned()` for the `Option`.
```rust
let recorder = {
    // `unwrap()` here is on the Mutex lock result, not the Option.
    let guard = self.rollout.lock().unwrap();
    guard.as_ref().cloned() // handle Option<RolloutRecorder> without unwrap
};
```

- Inline variables in format strings: Prefer brace interpolation over concatenation.
```rust
error!("Failed to create MCP connection manager: {e:#}");
warn!("{message}");
```


**DON'Ts**
- Don’t pre-generate a `session_id` before resume/new decides it: Avoid constructing clients with a provisional ID.
```rust
// ❌ Wrong: provisional ID may diverge from resumed session
let session_id = Uuid::new_v4();
let client = ModelClient::new(config.clone(), auth.clone(), provider.clone(), mre, mrs, session_id);

// ✅ Right: read from rollout result before building the client
let session_id = rollout_result.session_id;
let client = ModelClient::new(config.clone(), auth.clone(), provider.clone(), mre, mrs, session_id);
```

- Don’t run independent setup steps serially: Avoid awaiting each step in sequence when they don’t depend on each other.
```rust
// ❌ Wrong: sequential, higher startup latency
let rr = rollout_fut.await?;
let (mgr, failures) = McpConnectionManager::new(cfg.mcp_servers.clone()).await?;
let default_shell = shell::default_user_shell().await;

// ✅ Right: join them
let (rr, mcp_res, default_shell, _) = tokio::join!(rollout_fut, mcp_fut, default_shell_fut, history_meta_fut);
```

- Don’t surface errors before acknowledging the session: Avoid sending error events that arrive before `SessionConfigured`.
```rust
// ❌ Wrong order
tx_event.send(error_event).await?;
tx_event.send(session_configured_event).await?;

// ✅ Right order
tx_event.send(session_configured_event).await?;
tx_event.send_many(post_session_configured_error_events).await?;
```

- Don’t drop RolloutRecorder failures silently when not resuming: Always warn and send a user-facing error event.
```rust
// ❌ Wrong: ignores failure, no user signal
let recorder = RolloutRecorder::new(&config, sid, ui.clone()).await.ok();

// ✅ Right: warn + enqueue error for UI
let message = format!("failed to initialize rollout recorder: {e}");
post_session_configured_error_events.push(Event {
    id: INITIAL_SUBMIT_ID.to_owned(),
    msg: EventMsg::Error(ErrorEvent { message: message.clone() }),
});
warn!("{message}");
```

- Don’t make multiple `record_conversation_items` calls for initial context: Batch them.
```rust
// ❌ Wrong
sess.record_conversation_items(&[user_msg]).await;
sess.record_conversation_items(&[env_msg]).await;

// ✅ Right
sess.record_conversation_items(&[user_msg, env_msg]).await;
```

- Don’t confuse `Mutex` and `Option` unwrapping: Never `unwrap()` the `Option` inside the lock just to clone.
```rust
// ❌ Wrong: panics if None
let recorder = self.rollout.lock().unwrap().unwrap();

// ✅ Right
let recorder = {
    let guard = self.rollout.lock().unwrap();
    guard.as_ref().cloned()
};
```

- Don’t hide MCP client startup failures: Report each by name with context.
```rust
// ❌ Wrong: only logs
error!("Some MCP clients failed");

// ✅ Right: per-client surfaced to user
for (name, why) in failed_clients {
    let message = format!("MCP client '{name}' failed to start: {why:#}");
    post_session_configured_error_events.push(Event {
        id: INITIAL_SUBMIT_ID.to_owned(),
        msg: EventMsg::Error(ErrorEvent { message }),
    });
}
```