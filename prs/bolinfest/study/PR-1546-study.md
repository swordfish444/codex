**DOs**
- **Use Clear Type Names:** Prefer `TestCase` over ambiguous names like `Case`.
```rust
struct TestCase {
    name: &'static str,
    event: serde_json::Value,
    expected: Expected,
}
```

- **Run Each Case Independently:** Avoid a single looped test; extract a helper and write one test per case for better failure isolation.
```rust
use serde_json::json;

async fn assert_event(event: serde_json::Value, expected: Expected) {
    let mut evs = vec![event, json!({"type":"response.completed","response":{"id":"c","output":[]}})];
    let out = run_sse(evs).await;
    assert_eq!(out.len(), expected.len);
    assert!(expected.first.matches(&out[0]));
}

#[tokio::test]
async fn created_emits_created() {
    assert_event(json!({"type":"response.created","response":{}}),
                 Expected { first: Expectation::Created, len: 2 }).await;
}

#[tokio::test]
async fn output_item_done_emits_output() {
    assert_event(json!({"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}),
                 Expected { first: Expectation::OutputItemDone, len: 2 }).await;
}
```

- **Document Non‑Trivial Tests:** Add a concise doc comment explaining what the test validates.
```rust
/// Verifies that SSE events map to the correct `ResponseEvent`
/// and that a synthetic `response.completed` ends the stream.
#[tokio::test]
async fn created_emits_created() { /* ... */ }
```

- **Separate Inputs From Expectations:** Use distinct structs for what drives the test vs. what should happen.
```rust
struct EventArgs {
    name: &'static str,
    event: serde_json::Value,
}

struct Expected {
    first: Expectation,
    len: usize,
}
```

- **Prefer Typed Expectations Over Fn Pointers:** Use an enum with a matcher method instead of `fn(&ResponseEvent) -> bool`.
```rust
enum Expectation {
    Created,
    OutputItemDone,
    Completed,
}

impl Expectation {
    fn matches(&self, ev: &ResponseEvent) -> bool {
        match self {
            Expectation::Created => matches!(ev, ResponseEvent::Created),
            Expectation::OutputItemDone => matches!(ev, ResponseEvent::OutputItemDone(_)),
            Expectation::Completed => matches!(ev, ResponseEvent::Completed { .. }),
        }
    }
}
```

- **Remove Unneeded Lint Suppressions:** Drop `#[allow(dead_code)]` once helpers are used.
```rust
// Before:
// #[allow(dead_code)]
// pub fn load_sse_fixture(path: impl AsRef<Path>) -> String { ... }

// After:
pub fn load_sse_fixture(path: impl AsRef<std::path::Path>) -> String { /* used in tests */ }
```


**DON’Ts**
- **Don’t Use Ambiguous Names:** Avoid `struct Case`; it’s unclear and collides with other languages’ keywords.
```rust
// ❌
struct Case { /* ... */ }

// ✅
struct TestCase { /* ... */ }
```

- **Don’t Pack Many Cases Into One Test:** A failing early case hides later failures.
```rust
// ❌ Single test with loop:
#[tokio::test]
async fn event_kinds_in_one_go() {
    for c in cases {
        /* first failure stops here */
    }
}
```

- **Don’t Assert With Bare Function Pointers:** They’re less expressive and harder to extend.
```rust
// ❌
expect_first: fn(&ResponseEvent) -> bool

// ✅
expected: Expected { first: Expectation::Created, len: 2 }
```

- **Don’t Leave Dead-Code Allows Behind:** If a helper is referenced, remove `#[allow(dead_code)]`.
```rust
// ❌
#[allow(dead_code)]
pub fn load_sse_fixture_with_id(...) -> String { /* actually used */ }

// ✅
pub fn load_sse_fixture_with_id(...) -> String { /* used */ }
```

- **Don’t Skip Test Docs When Behavior Is Subtle:** Undocumented tests slow reviews and regressions hunts.
```rust
// ❌
#[tokio::test]
async fn table_driven_event_kinds() { /* ... */ }

// ✅
/** Explains mapping and termination behavior. */
#[tokio::test]
async fn table_driven_event_kinds() { /* ... */ }
```