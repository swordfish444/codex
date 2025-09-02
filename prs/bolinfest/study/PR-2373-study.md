**DOs**
- Imports at top: Keep all `use` statements consolidated at the file top.
```rust
use std::{
    io,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Duration,
};
```

- Duration for timeouts: Use `Duration` and name fields without units.
```rust
pub struct ServerOptions {
    pub login_timeout: Option<Duration>,
    // ...
}

// Init with explicit units
let opts = ServerOptions {
    login_timeout: Some(Duration::from_secs(10 * 60)),
    ..ServerOptions::new(home, client_id)
};
```

- Cancellable timeout watcher: Disarm the timer when login completes; use `compare_exchange` and `server.unblock()`.
```rust
fn spawn_timeout_watcher(
    done_rx: std::sync::mpsc::Receiver<()>,
    timeout: Duration,
    shutdown: Arc<AtomicBool>,
    timed_out: Arc<AtomicBool>,
    server: Arc<tiny_http::Server>,
) {
    std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err()
            && shutdown.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
        {
            timed_out.store(true, Ordering::SeqCst);
            server.unblock(); // promptly exit recv()
        }
    });
}
```

- Share with Arc, not Clone: Wrap server in `Arc` and expose a minimal `ShutdownHandle`.
```rust
pub struct ShutdownHandle {
    shutdown: Arc<AtomicBool>,
    server: Arc<tiny_http::Server>,
}
impl ShutdownHandle {
    pub fn cancel(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.server.unblock();
    }
}
```

- Handle recv() errors with shutdown flag: Treat `unblock()`-caused errors as graceful exit.
```rust
let server = server.clone(); // Arc<Server>
let handle = std::thread::spawn(move || -> io::Result<()> {
    while !shutdown.load(Ordering::SeqCst) {
        match server.recv() {
            Ok(req) => { /* handle request */ }
            Err(e) => {
                if shutdown.load(Ordering::SeqCst) { break; }
                return Err(io::Error::other(e));
            }
        }
    }
    Ok(())
});
```

- Keep the message loop non-blocking: Reply immediately, then monitor completion in background.
```rust
let login_id = Uuid::new_v4();
outgoing.send_response(request_id, LoginChatGptResponse {
    login_id,
    auth_url: server.auth_url.clone(),
}).await;

let outgoing = outgoing.clone();
tokio::spawn(async move {
    let res = tokio::task::spawn_blocking(move || server.block_until_done()).await;
    let (success, error) = match res {
        Ok(Ok(())) => (true, None),
        Ok(Err(e)) => (false, Some(format!("Login server error: {e}"))),
        Err(join)  => (false, Some(format!("Join error: {join}"))),
    };
    outgoing.send_notification(OutgoingNotification {
        method: LOGIN_CHATGPT_COMPLETE_EVENT.to_string(),
        params: serde_json::to_value(LoginChatGptCompleteNotification { login_id, success, error }).ok(),
    }).await;
});
```

- Single active login: Store one `ActiveLogin` in `Arc<Mutex<Option<_>>>`; cancel previous and drop the lock before awaits.
```rust
struct ActiveLogin { shutdown: ShutdownHandle, login_id: Uuid }

{
    let mut guard = self.active_login.lock().await;
    if let Some(prev) = guard.take() { prev.shutdown.cancel(); }
    *guard = Some(ActiveLogin { shutdown: server.cancel_handle(), login_id });
} // lock released here
```

- Explicit cancel API: Validate `login_id` and respond clearly.
```rust
let active = { self.active_login.lock().await.take() };
match active {
    Some(a) if a.login_id == login_id => {
        a.shutdown.cancel();
        outgoing.send_response(request_id, CancelLoginChatGptResponse {}).await;
    }
    _ => {
        outgoing.send_error(request_id, JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("login id not found: {login_id}"),
            data: None,
        }).await;
    }
}
```

- Unify send path: Use a small enum to funnel response vs. error into one send site.
```rust
enum Reply<T> { Response(T), Error(JSONRPCErrorError) }

match reply {
    Reply::Response(v) => outgoing.send_response(request_id, v).await,
    Reply::Error(e)    => outgoing.send_error(request_id, e).await,
}
```

**DON’Ts**
- Mid-file imports: Don’t add `use` statements below type/impl blocks.
```rust
// ❌ Avoid
// ... many lines later ...
use std::mem::ManuallyDrop;
```

- Clone owning server types: Don’t implement `Clone` for types that own threads/handles.
```rust
// ❌ Avoid
#[derive(Clone)]
pub struct LoginServer { server_handle: std::thread::JoinHandle<()> }
```

- Timeout as raw integers: Don’t use `Option<u64>` with names like `*_secs`.
```rust
// ❌ Avoid
pub struct ServerOptions { pub login_timeout_secs: Option<u64> }
```

- Sleep-only timers: Don’t spawn a thread that always sleeps to completion.
```rust
// ❌ Avoid
std::thread::spawn(move || {
    std::thread::sleep(Duration::from_secs(secs));
    shutdown.store(true, Ordering::SeqCst);
    // timer can’t be disarmed; lingers after success
});
```

- Dummy HTTP “nudge”: Don’t poke localhost to unblock; use `server.unblock()`.
```rust
// ❌ Avoid
let _ = std::net::TcpStream::connect(format!("127.0.0.1:{actual_port}"));
```

- Block the message processor: Don’t wait for login to finish before replying.
```rust
// ❌ Avoid
self.login_chatgpt(request_id).await; // performs blocking join inside
```

- Multiple concurrent logins: Don’t track a `HashMap<Uuid, ActiveLogin>` unless truly needed.
```rust
// ❌ Avoid
active_logins: Arc<Mutex<HashMap<Uuid, ActiveLogin>>>
```

- Hold locks across awaits: Don’t keep a mutex guard while awaiting I/O.
```rust
// ❌ Avoid
let mut guard = self.active_login.lock().await;
outgoing.send_response(request_id, resp).await; // lock held here
```

- Wildcard where `None` is intended: Prefer explicit `None` matches.
```rust
// ❌ Avoid
match guard.as_ref().map(|l| l.login_id) { _ => /* ... */ }

// ✅ Prefer
match guard.as_ref().map(|l| l.login_id) {
    None => { /* ... */ }
    Some(id) => { /* ... */ }
}
```

- Over-testing serialization: Don’t unit-test JSON for every message type; keep tests focused.
```rust
// ❌ Avoid boilerplate serialization snapshot for each enum variant
// Prefer a small, representative set to verify tagging/shape.
```