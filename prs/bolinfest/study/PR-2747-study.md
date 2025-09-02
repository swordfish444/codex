**DOs**
- Move the sender: Move `mpsc::Sender` into the producer task so it drops when the task ends, allowing the receiver to finish.
```rust
use tokio::sync::mpsc;

let (tx, mut rx) = mpsc::channel::<String>(16);

let stdin_task = tokio::spawn(async move {
    let _ = tx.send("line".into()).await;
}); // tx drops here

let processor = tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
        // handle msg
    } // exits when all senders drop
});
```

- Drop before join: If the parent holds an extra sender (e.g., for setup), drop it explicitly before awaiting joins.
```rust
let (tx, mut rx) = mpsc::channel::<()>(1);
let tx_parent = tx.clone();

let t = tokio::spawn(async move {
    let _ = tx.send(()).await;
});

// Critical: close the channel on the parent side
drop(tx_parent);

let _ = tokio::join!(t);
```

- Rely on move capture: Let `async move` capture needed variables; avoid needless rebindings inside `tokio::spawn`.
```rust
let (tx, _rx) = tokio::sync::mpsc::channel::<String>(16);

let task = tokio::spawn(async move {
    let _ = tx.send("ok".into()).await; // use tx directly
});
```

- Bound receiver loops: Use `while let Some(...)` to exit cleanly when the channel closes.
```rust
let processor = tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
        // process msg
    } // channel closed => loop ends
});
```

- Clone intentionally: Only clone when you truly need multiple producers, and ensure each clone is dropped.
```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
let tx2 = tx.clone();

let a = tokio::spawn(async move { let _ = tx.send("a".into()).await; });
let b = tokio::spawn(async move { let _ = tx2.send("b".into()).await; });

let _ = tokio::join!(a, b);

while let Some(_msg) = rx.recv().await {}
```

**DON’Ts**
- Clone inside the task needlessly: Don’t create an extra `Sender` clone in a spawned task when the closure can move the original.
```rust
// Anti-pattern: leaves an extra Sender alive outside the task
let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);

let stdin_task = tokio::spawn({
    let tx_clone = tx.clone(); // unnecessary clone
    async move {
        let _ = tx_clone.send("line".into()).await;
    }
});

// If `tx` remains alive here, `rx` may never see closure.
```

- Keep spare senders across join: Don’t hold a `Sender` in the parent scope while awaiting task completion.
```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
let tx_main = tx.clone();

let t = tokio::spawn(async move { let _ = tx.send(()).await; });

// Anti-pattern: join while `tx_main` is still alive
let _ = tokio::join!(t); // receiver may never finish
```

- Self-assign in closures: Don’t write `let tx = tx;` inside the `tokio::spawn` block; remove the line and rely on move capture.
```rust
// Anti-pattern
let task = tokio::spawn({
    let tx = tx; // redundant; just omit this line
    async move { /* use tx */ }
});
```

- Create accidental long-lived owners: Don’t store the sender in a struct or outer variable that outlives the worker tasks unless you explicitly manage its drop.
```rust
struct State { tx: tokio::sync::mpsc::Sender<String> }
let state = State { tx }; // long-lived owner
// … later …
let _ = tokio::join!(worker1, worker2); // may hang if `state.tx` is still alive
```

- Assume receiver stops without closure: Don’t use unbounded loops that ignore channel closure; always handle `None` from `recv()`.
```rust
// Anti-pattern
loop {
    // `recv()` returns Option<T>; ignoring None risks hangs or errors
    if let Some(msg) = rx.recv().await {
        // process
    } else {
        break; // handle closure explicitly
    }
}
```