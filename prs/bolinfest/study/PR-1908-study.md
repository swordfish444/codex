**DOs**

- **Document Guarantees**: Add a docstring stating the helper always emits begin/end and is used for both initial exec and sandbox retry.
```rust
/// Runs the exec tool call and ALWAYS emits begin and end events,
/// even if this returns Err. Used for both initial run and sandbox retry.
async fn run_exec_with_events<'a>(...) -> Result<ExecToolCallOutput> { ... }
```

- **Centralize Event Emission**: Wrap `process_exec_tool_call` in a single helper that handles errors and still emits `on_exec_command_end`.
```rust
self.on_exec_command_begin(turn_diff_tracker, begin_ctx.clone()).await;

let result = process_exec_tool_call(...).await;

let fallback;
let output = match &result {
    Ok(o) => o,
    Err(e) => {
        fallback = ExecToolCallOutput {
            exit_code: -1,
            stdout: String::new(),
            stderr: get_error_message_ui(e),
            duration: Duration::default(),
        };
        &fallback
    }
};

self.on_exec_command_end(turn_diff_tracker, &sub_id, &call_id, output, is_apply_patch).await;

result
```

- **Use Top‑Level Imports**: Import commonly used types to simplify signatures and struct fields.
```rust
use crate::exec::{ExecParams, SandboxType, StdoutStream};
use crate::sandbox::SandboxPolicy;

pub struct ExecInvokeArgs<'a> {
    pub params: ExecParams,
    pub sandbox_type: SandboxType,
    pub ctrl_c: Arc<Notify>,
    pub sandbox_policy: &'a SandboxPolicy,
    pub codex_linux_sandbox_exe: &'a Option<PathBuf>,
    pub stdout_stream: Option<StdoutStream>,
}
```

- **Defer “End” Until After Sandbox Resolution**: Call the same helper for the initial run and the escalated retry so “end” is emitted once per attempt, after a definitive result.
```rust
// Initial
let output_result = sess.run_exec_with_events(turn_diff, ctx.clone(), args_initial).await;

// Retry (no sandbox)
let retry_result  = sess.run_exec_with_events(turn_diff, ctx.clone(), args_retry_none).await;
```

- **Align Error Text With UI**: Show the same error content in stderr and response using `get_error_message_ui`.
```rust
pub fn get_error_message_ui(e: &CodexErr) -> String {
    match e {
        CodexErr::Sandbox(SandboxErr::Denied(_, _, stderr)) => stderr.to_string(),
        _ => e.to_string(),
    }
}
```

- **Build Consistent Responses**: Include `success` and formatted content; clone IDs where needed.
```rust
let is_success = output.exit_code == 0;
let content = format_exec_output(
    if is_success { &output.stdout } else { &output.stderr },
    output.exit_code,
    output.duration,
);

ResponseInputItem::FunctionCallOutput {
    call_id: call_id.clone(),
    output: FunctionCallOutputPayload { content, success: Some(is_success) },
}
```

- **Inline Variable Formatting**: Prefer inlined `{var}` over `"{}"`.
```rust
let msg = format!("retry failed: {e}");
let msg2 = format!("execution error: {e}");
```

**DON’Ts**

- **Don’t Manually Pair Begin/End At Call Sites**: Avoid duplicating `on_exec_command_begin/end` around each tool call.
```rust
// ❌ Anti-pattern
sess.on_exec_command_begin(...).await;
let res = process_exec_tool_call(...).await;
sess.on_exec_command_end(..., res.as_ref().ok_or(&fallback)?, ...).await;
```

- **Don’t Over‑Qualify Types In Structs**: Skip verbose `crate::exec::...` when a `use` makes it clearer.
```rust
// ❌ Anti-pattern
pub struct ExecInvokeArgs<'a> {
    pub params: crate::exec::ExecParams,
    pub sandbox_type: crate::exec::SandboxType,
    // ...
}
```

- **Don’t Diverge UI And Stderr Messages**: Don’t surface a generic error to the user while logging a different stderr.
```rust
// ❌ Anti-pattern
let ui_text = format!("retry failed: {e}");
let stderr = e.to_string(); // different from sandbox-denied stderr body
```

- **Don’t Use Unstable `let` Chains**: Replace with stable patterns (`match`, `if let`, tuple matching).
```rust
// ❌ Anti-pattern (unstable let chains)
// if let Some(a) = x && let Some(b) = y { ... }

// ✅ Stable alternative
if let (Some(a), Some(b)) = (x, y) {
    // ...
}
```

- **Don’t Emit “End” Before Sandbox Decision**: Don’t finalize the event stream until the sandbox approval path is resolved.
```rust
// ❌ Anti-pattern
// Emit end here, then later retry and emit another end
sess.on_exec_command_end(...).await; // too early
```