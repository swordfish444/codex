Exec timeouts and pipe draining: practical takeaways from bolinfest’s review

**DOs**
- Enforce bounded waits on stdout/stderr drains: add a short timeout so open FDs from grandchildren can’t hang reads.
- Prefer `tokio::time::timeout` over `select! + sleep`: it’s clearer and cancels the awaited future when elapsed.
- Abort reader tasks on timeout: call `handle.abort()` to break pending `read()` on inherited pipes.
- Propagate errors meaningfully: convert `JoinError` to `io::Error` with `Error::other` instead of `??` obscurity.
- Keep `JoinHandle` mutable: you need `&mut` to both await and `abort()` it.
- Use a small, documented drain window: e.g., 2s for local pipes; explain “why” in comments.

```rust
// Recommended: bounded wait for IO drain with clear cancellation.
use std::{io, time::Duration};
use tokio::{task::JoinHandle, time};

const IO_DRAIN_TIMEOUT_MS: u64 = 2_000; // Short window; avoids hangs on inherited FDs.

async fn await_with_timeout(
    handle: &mut JoinHandle<io::Result<Vec<u8>>>,
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    match time::timeout(timeout, &mut *handle).await {
        Ok(join_res) => match join_res {
            Ok(io_res) => io_res,
            Err(join_err) => Err(io::Error::other(join_err)),
        },
        Err(_elapsed) => {
            // Child may be dead, but grandchildren still hold pipe FDs.
            handle.abort(); // Break any pending read() so we don’t hang forever.
            Ok(Vec::new())  // Return empty on timeout; logs can capture that we timed out.
        }
    }
}

// Usage
let mut stdout_handle = stdout_handle;
let mut stderr_handle = stderr_handle;

let stdout = await_with_timeout(
    &mut stdout_handle,
    Duration::from_millis(IO_DRAIN_TIMEOUT_MS),
).await?;
let stderr = await_with_timeout(
    &mut stderr_handle,
    Duration::from_millis(IO_DRAIN_TIMEOUT_MS),
).await?;
```

**DON’Ts**
- Don’t await drain tasks unbounded: `handle.await??` can hang if grandchildren keep stdout/stderr open.
- Don’t rely on child death to close pipes: inherited FDs outlive the killed parent.
- Don’t hand-roll `select! + sleep` timeouts: it’s more verbose and easier to get cancellation semantics wrong.
- Don’t ignore `JoinError`: surface it as `io::Error` so callers can act.
- Don’t forget mutability: `abort()` requires a mutable handle.
- Don’t pick long drain timeouts: small, conservative windows reduce tail-latency and dead-time.

```rust
// Anti-patterns (avoid):

// 1) Unbounded wait: can hang forever if pipes stay open.
let stdout = stdout_handle.await??;
let stderr = stderr_handle.await??;

// 2) Hand-rolled timeout with select + sleep: harder to reason about cancellation.
tokio::select! {
    res = &mut handle => { /* ambiguous error propagation and no explicit abort */ }
    _ = tokio::time::sleep(Duration::from_secs(2)) => {
        // Future is still pending unless you remember to abort.
    }
}
```