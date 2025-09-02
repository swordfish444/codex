**DOs**

- **Check Simple Conditions Before Locking:** Gate on cheap checks like `query.is_empty()` or identical queries without taking the mutex.
```rust
// Good: no lock held for trivial early-exit
if query.is_empty() {
    let max_total = MAX_FILE_SEARCH_RESULTS.get();
    let matches = collect_top_level_suggestions(&self.search_dir, max_total);
    self.app_tx.send(AppEvent::FileSearchResult {
        query: String::new(),
        matches,
    });
    return;
}

// Only now take the lock
let mut st = self.state.lock().unwrap();
if query == st.latest_query {
    return;
}
```

- **Model State Explicitly (avoid overloading):** Prefer clear flags or enums over `Option<String>` when representing “never shown” vs “empty string”.
```rust
// Prefer explicit fields
struct SearchState {
    latest_query: String,
    has_shown_popup: bool, // instead of latest_query: Option<String>
}
```

- **Keep Code Paths Symmetric:** Make empty and non-empty query flows update/search state consistently to preserve invariants.
```rust
fn handle_query(&self, query: String) {
    if query.is_empty() {
        self.finish_empty_query(&query);      // updates scheduling state as needed
    } else {
        self.start_non_empty_search(query);   // sets active_search + cancellation_token
    }
}

fn finish_empty_query(&self, query: &str) {
    // Clear any scheduled search consistently
    let mut st = self.state.lock().unwrap();
    st.is_search_scheduled = false;
    st.active_search = None;
    drop(st);

    let max_total = MAX_FILE_SEARCH_RESULTS.get();
    let matches = collect_top_level_suggestions(&self.search_dir, max_total);
    self.app_tx.send(AppEvent::FileSearchResult {
        query: query.to_string(),
        matches,
    });
}
```

- **Short-Circuit Unchanged Inputs:** Avoid re-scheduling when the query did not change; combine with the empty check if appropriate.
```rust
let unchanged = {
    let st = self.state.lock().unwrap();
    query == st.latest_query
};
if unchanged {
    return;
}
```

- **Put Imports At The Top:** Keep `use` statements at module scope; use descriptive types.
```rust
use std::collections::HashSet;
use std::fs;
```

- **Use Descriptive Names:** Prefer self-explanatory identifiers over two-letter names in filesystem loops.
```rust
let mut seen: HashSet<String> = HashSet::new();

if let Ok(dir_iter) = fs::read_dir(cwd) {
    for dir_entry in dir_iter.flatten() {
        let entry_path = dir_entry.path();
        let Some(name) = entry_path.strip_prefix(cwd).ok().and_then(|p| p.to_str()) else { continue };
        // ...
    }
}
```

**DON’Ts**

- **Don’t Hold Locks During I/O or Heavy Work:** Copy out what you need, drop the lock, then perform filesystem or compute operations.
```rust
// Anti-pattern: lock held across I/O
let mut st = self.state.lock().unwrap();
for entry in std::fs::read_dir(&self.search_dir).unwrap() { /* ... */ }

// Better:
let search_dir = self.search_dir.clone();
drop(st);
for entry in std::fs::read_dir(search_dir).unwrap() { /* ... */ }
```

- **Don’t Introduce APIs Without Clear Need:** Avoid adding methods like `reset()` unless they’re necessary, consistently used, and tested.
```rust
// Instead of adding a broad reset(), cancel the active search via existing state.
let mut st = self.state.lock().unwrap();
if let Some(active) = st.active_search.take() {
    active.cancellation_token.store(true, std::sync::atomic::Ordering::Relaxed);
}
st.is_search_scheduled = false;
```

- **Don’t Overload `Option<String>` For Multiple States:** `None` vs `Some("")` is ambiguous; model intent explicitly.
```rust
// Avoid:
latest_query: Option<String>

// Prefer:
latest_query: String,
has_shown_popup: bool,
```

- **Don’t Put `use` Inside Functions For Common Types:** Keep module imports centralized; local `use` obscures dependencies.
```rust
// Avoid inside fn:
fn collect_top_level_suggestions(...) {
    use std::collections::HashSet; // <-- move to top
}
```

- **Don’t Diverge State Between Code Paths:** Ensure the “empty query” fast path doesn’t skip essential bookkeeping done by the normal path.
```rust
// Avoid: empty path bypasses state updates
if query.is_empty() {
    // send results and return; state not updated
    return;
}

// Prefer: both paths maintain scheduler/active_search invariants
handle_query(query);
```