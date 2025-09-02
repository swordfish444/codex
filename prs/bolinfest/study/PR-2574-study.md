**DOs**
- **Prefer Streamable Shell**: Use `exec_command` + `write_stdin` for long‑running or interactive commands instead of `shell`.
- **Validate Tool Args**: Parse JSON with `serde_json` and return a clear failure payload on error.
- **Wire Through Config**: Gate the new tools behind `experimental_use_exec_command_tool` and plumb it into `ToolsConfig`.
- **Surface Tools Correctly**: Add both tools to the Responses API tool list when the flag is enabled.
- **Use PTY Safely**: Spawn the child in a PTY via `portable-pty`; run blocking IO in `spawn_blocking`.
- **Handle EINTR/WouldBlock**: Retry reads on `Interrupted`; back off briefly on `WouldBlock`.
- **Avoid Locking Across Awaits**: Copy handles you need, then release the lock before `await`.
- **Time‑Bound Output**: Collect output up to `yield_time_ms` using `timeout`; bias toward noticing process exit.
- **Truncate Middle, UTF‑8 Safe**: Prefer newline boundaries; never split UTF‑8; include a clear truncation marker with token estimate.
- **Report Status Clearly**: Include wall time, exit/executing status, and a truncation warning in the returned text.
- **Clean Up Robustly**: On drop, kill the child and abort background tasks.
- **Write Stdin Incrementally**: Support control chars (e.g., Ctrl‑C) and empty writes to poll output.
- **Make Tests Resilient**: Skip PTY‑restricted environments (e.g., “openpty” or “Operation not permitted”) instead of failing.

```rust
// Validate tool args and return failures clearly.
match serde_json::from_str::<ExecCommandParams>(&arguments) {
    Ok(params) => {
        let result = SESSION_MANAGER.handle_exec_command_request(params).await;
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: result_into_payload(result),
        };
    }
    Err(e) => {
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("failed to parse function arguments: {e}"),
                success: Some(false),
            },
        };
    }
}
```

```rust
// ToolsConfig wiring (enable streamable shell when the experiment is on).
let tools_config = ToolsConfig::new(
    model_family,
    approval_policy,
    sandbox_policy.clone(),
    /*include_plan_tool*/ config.include_plan_tool,
    /*include_apply_patch_tool*/ config.include_apply_patch_tool,
    /*use_streamable_shell_tool*/ config.use_experimental_streamable_shell_tool,
);
```

```rust
// Surface the tools in Responses API.
tools.push(OpenAiTool::Function(create_exec_command_tool_for_responses_api()));
tools.push(OpenAiTool::Function(create_write_stdin_tool_for_responses_api()));
```

```rust
// Spawn PTY + child; perform blocking IO in blocking threads and handle EINTR/WouldBlock.
use std::io::{Read, ErrorKind};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::time::Duration;

let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;
let mut cmd = CommandBuilder::new("/bin/bash");
cmd.arg("-lc").arg(cmd_string);
let mut child = pair.slave.spawn_command(cmd)?;
let killer = child.clone_killer();

let mut reader = pair.master.try_clone_reader()?;
let tx = output_tx.clone();
let reader_handle = tokio::task::spawn_blocking(move || {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => { let _ = tx.send(buf[..n].to_vec()); }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => break,
        }
    }
});
```

```rust
// Don’t hold locks across await: copy handles then drop the lock.
let (writer_tx, mut output_rx) = {
    let sessions = self.sessions.lock().await;
    let sess = sessions.get(&session_id).ok_or_else(|| format!("unknown session id {}", session_id.0))?;
    (sess.writer_sender(), sess.output_receiver())
};
```

```rust
// Time-bounded collection with exit bias and post-exit grace drain.
use tokio::time::{timeout, Instant, Duration};
let start = Instant::now();
let deadline = start + Duration::from_millis(yield_time_ms);
let mut collected = Vec::with_capacity(4096);
let mut exit_code: Option<i32> = None;

loop {
    if Instant::now() >= deadline { break; }
    let remaining = deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        biased;
        exit = &mut exit_rx => {
            exit_code = exit.ok();
            let grace_deadline = Instant::now() + Duration::from_millis(25);
            while Instant::now() < grace_deadline {
                if let Ok(Ok(chunk)) = timeout(Duration::from_millis(1), output_rx.recv()).await {
                    collected.extend_from_slice(&chunk);
                } else { break; }
            }
            break;
        }
        chunk = timeout(remaining, output_rx.recv()) => {
            if let Ok(Ok(chunk)) = chunk { collected.extend_from_slice(&chunk); } else { break; }
        }
    }
}
```

```rust
// UTF-8 safe middle truncation that prefers newline boundaries.
fn truncate_middle(s: &str, max_bytes: usize) -> (String, Option<u64>) {
    if s.len() <= max_bytes { return (s.to_string(), None); }
    let est_tokens = (s.len() as u64).div_ceil(4);
    let marker = format!("…{est_tokens} tokens truncated…");
    if max_bytes <= marker.len() {
        return (marker, Some(est_tokens));
    }
    let keep = max_bytes - marker.len();
    let left = keep / 2;
    let right = keep - left;

    let prefix_end = s[..s.len().min(left)]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or_else(|| {
            let mut e = left.min(s.len());
            while e > 0 && !s.is_char_boundary(e) { e -= 1; }
            e
        });

    let start_tail = s.len().saturating_sub(right);
    let suffix_start = s[start_tail..]
        .find('\n')
        .map(|i| start_tail + i + 1)
        .unwrap_or_else(|| {
            let mut i = start_tail.min(s.len());
            while i < s.len() && !s.is_char_boundary(i) { i += 1; }
            i
        });

    let mut out = String::with_capacity(max_bytes);
    out.push_str(&s[..prefix_end]);
    out.push_str(&marker);
    out.push('\n');
    out.push_str(&s[suffix_start..]);
    (out, Some(est_tokens))
}
```

```rust
// Clear status text with inline variables.
impl ExecCommandOutput {
    fn to_text_output(&self) -> String {
        let wall = self.wall_time.as_secs_f32();
        let status = match self.exit_status {
            ExitStatus::Exited(code) => format!("Process exited with code {code}"),
            ExitStatus::Ongoing(id) => format!("Process running with session ID {}", id.0),
        };
        let trunc = match self.original_token_count {
            Some(tokens) => format!("\nWarning: truncated output (original token count: {tokens})"),
            None => String::new(),
        };
        format!(
            "Wall time: {wall:.3} seconds\n{status}{trunc}\nOutput:\n{}",
            self.output
        )
    }
}
```

```rust
// Drop: kill child and abort tasks best-effort.
impl Drop for ExecCommandSession {
    fn drop(&mut self) {
        if let Ok(mut killer_opt) = self.killer.lock() {
            if let Some(mut killer) = killer_opt.take() { let _ = killer.kill(); }
        }
        for handle in [&self.reader_handle, &self.writer_handle, &self.wait_handle] {
            if let Ok(mut h) = handle.lock() {
                if let Some(j) = h.take() { j.abort(); }
            }
        }
    }
}
```

```rust
// Write to stdin (including control chars) and poll for output.
let _ = writer_tx.send("help\n".as_bytes().to_vec()).await;
let _ = writer_tx.send("\u{0003}".as_bytes().to_vec()).await; // Ctrl-C
// Poll without writing: set chars="" and just collect output for yield_time_ms.
```

```rust
// Exec then incremental interaction example.
let start = SESSION_MANAGER.handle_exec_command_request(ExecCommandParams {
    cmd: "python3 -i".to_string(),
    yield_time_ms: 2_000,
    max_output_tokens: 10_000,
    shell: "/bin/bash".to_string(),
    login: true,
}).await?;

let session_id = match start.exit_status {
    ExitStatus::Ongoing(id) => id,
    ExitStatus::Exited(code) => panic!("unexpected exit: {code}"),
};

let after = SESSION_MANAGER.handle_write_stdin_request(WriteStdinParams {
    session_id,
    chars: "print(1+1)\n".to_string(),
    yield_time_ms: 750,
    max_output_tokens: 256,
}).await?;
```

```rust
// Test: skip when PTY is restricted.
let out = match session_manager.handle_exec_command_request(params).await {
    Ok(v) => v,
    Err(e) => {
        if e.contains("openpty") || e.contains("Operation not permitted") {
            eprintln!("skipping test due to restricted PTY: {e}");
            return;
        }
        panic!("unexpected exec error: {e}");
    }
};
```

```toml
# codex-rs config (e.g., in profile .toml)
experimental_use_exec_command_tool = true
```

```rust
// Tool schemas: strict parameters with no additional properties.
ResponsesApiTool {
    name: "exec_command".to_string(),
    description: "Execute shell commands on the local machine with streaming output.".to_string(),
    strict: false,
    parameters: JsonSchema::Object {
        properties,
        required: Some(vec!["cmd".to_string()]),
        additional_properties: Some(false),
    },
}
```


**DON’Ts**
- **Don’t Block Async Threads**: Avoid direct blocking reads/writes on async runtimes; use `spawn_blocking`.
- **Don’t Hold Mutexes Across Awaits**: Never keep the sessions map locked while awaiting channel ops or timeouts.
- **Don’t Drop Output Chunks Blindly**: Don’t per‑chunk truncate or trim only the end; truncate in the middle after collection.
- **Don’t Break UTF‑8**: Never slice strings without respecting char boundaries; prefer newline cuts when possible.
- **Don’t Ignore Exit Races**: Don’t miss late output on process exit; add a brief post‑exit grace drain.
- **Don’t Leak Processes**: Don’t rely on `.wait()` alone; hold a `ChildKiller` and terminate on drop.
- **Don’t Assume PTY Availability**: Don’t fail tests in restricted sandboxes; skip when PTY cannot be created.
- **Don’t Over‑Allocate**: Don’t preallocate huge buffers; start modestly and only truncate at the end.
- **Don’t Forget Both Tools**: Don’t expose `exec_command` without `write_stdin` when enabling Streamable Shell.
- **Don’t Allow Loose Schemas**: Don’t set `additional_properties = true`; require the minimal fields.
- **Don’t Hide Truncation**: Don’t silently cut output; include a visible marker and token estimate.
- **Don’t Hard‑Code Local Shell**: Don’t force Local Shell when the streamable flag is on; select `StreamableShell` in `ToolsConfig`.