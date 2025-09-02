**MCP Tool Naming: DOs and DON'Ts**

**DOs**
- **Use A Short, Allowed Delimiter:** Pick from `[A-Za-z0-9_-]` and keep it tiny.
```rust
const DELIM: &str = "__"; // short and allowed
```

- **Keep Encode/Parse Symmetric:** Ensure round-trips without data loss.
```rust
const DELIM: &str = "__";

fn fully_qualified_tool_name(server: &str, tool: &str) -> String {
    format!("{server}{DELIM}{tool}")
}

fn try_parse_fully_qualified_tool_name(s: &str) -> Option<(String, String)> {
    let (server, tool) = s.split_once(DELIM)?;
    if server.is_empty() || tool.is_empty() { return None; }
    Some((server.to_string(), tool.to_string()))
}
```

- **Enforce The 64-Char Limit Deterministically:** Truncate with a stable hash suffix when needed.
```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const MAX_LEN: usize = 64;
const DELIM: &str = "__";

fn hash_suffix(server: &str, tool: &str, n: usize) -> String {
    let mut h = DefaultHasher::new();
    (server, tool).hash(&mut h);
    format!("{:016x}", h.finish())[..n.min(16)].to_string()
}

fn bounded_name(server: &str, tool: &str) -> String {
    let base = format!("{server}{DELIM}{tool}");
    if base.len() <= MAX_LEN {
        return base;
    }
    let suffix = hash_suffix(server, tool, 8); // 8 hex chars
    let budget = MAX_LEN - DELIM.len() - 1 - suffix.len(); // 1 for '-'
    let s_keep = budget / 2;
    let t_keep = budget - s_keep;
    let s = server.chars().take(s_keep).collect::<String>();
    let t = tool.chars().take(t_keep).collect::<String>();
    format!("{s}{DELIM}{t}-{suffix}")
}
```

- **Sanitize Invalid Characters:** Replace anything outside the allowed set.
```rust
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}
```

- **Defer Full Qualification Until Collision:** Prefer readable names; qualify only duplicates.
```rust
use std::collections::{HashMap, HashSet};

fn qualify_on_collision(server_tools: &[(String, Vec<String>)]) -> HashMap<String, (String, String)> {
    let mut counts = HashMap::<String, usize>::new();
    for (_, tools) in server_tools {
        for t in tools { *counts.entry(t.clone()).or_default() += 1; }
    }
    let mut out = HashMap::new();
    for (server, tools) in server_tools {
        for tool in tools {
            let candidate = if counts[tool] > 1 { format!("{server}{DELIM}{tool}") } else { tool.clone() };
            out.insert(candidate, (server.clone(), tool.clone()));
        }
    }
    out
}
```

- **Use A Registry Instead Of Reverse-Parsing Hashed Names:** Map display names to the original (server, tool).
```rust
use std::collections::HashMap;

struct NameRegistry {
    display_to_key: HashMap<String, (String, String)>,
}

impl NameRegistry {
    fn register(&mut self, server: &str, tool: &str) -> String {
        let display = bounded_name(server, tool); // includes hashing if needed
        self.display_to_key.insert(display.clone(), (server.to_string(), tool.to_string()));
        display
    }
    fn resolve(&self, display: &str) -> Option<(String, String)> {
        self.display_to_key.get(display).cloned()
    }
}
```

- **Log When Fallbacks Apply:** Make truncation/hashing visible and traceable.
```rust
let original = format!("{server}{DELIM}{tool}");
let display = bounded_name(server, tool);
if display != original {
    tracing::info!("Shortened tool name from '{original}' to '{display}' to fit 64-char limit");
}
```

- **Test The Contract:** Validate delimiter, symmetry, and length bounds.
```rust
#[test]
fn fq_round_trip_and_length() {
    assert_eq!(DELIM.len(), 2);
    let s = fully_qualified_tool_name("srv", "tool");
    assert_eq!(try_parse_fully_qualified_tool_name(&s), Some(("srv".into(), "tool".into())));
    assert!(bounded_name("a", &"b".repeat(80)).len() <= MAX_LEN);
}
```

**DON'Ts**
- **Break Encode/Parse Symmetry:** Don’t emit names that `try_parse…` cannot faithfully decode.
```rust
// Anti-pattern: data loss by naive truncation; parse cannot restore originals.
fn bad_name(server: &str, tool: &str) -> String {
    let s = format!("{server}{DELIM}{tool}");
    s.chars().take(64).collect() // breaks round-trip
}
```

- **Use Long Delimiters:** Don’t waste budget on decorative separators.
```rust
// Anti-pattern: long delimiter reduces effective name space.
const DELIM: &str = "__OAI_CODEX_MCP__"; // too long
```

- **Silently Truncate Without Disambiguation:** Don’t drop uniqueness when shortening.
```rust
// Anti-pattern: no hash suffix; collisions likely.
fn truncate_only(server: &str, tool: &str, max: usize) -> String {
    let base = format!("{server}{DELIM}{tool}");
    base.chars().take(max).collect()
}
```

- **Fully Qualify Everything Upfront:** Don’t reduce readability and risk hitting limits unnecessarily.
```rust
// Anti-pattern: always '{server}__{tool}' even when tool names are unique.
let display = format!("{server}{DELIM}{tool}"); // do this only on collision
```

- **Skip Recoverable Tools:** Don’t drop a tool solely because names are long—shorten deterministically instead.
```rust
// Anti-pattern: skipping instead of applying a bounded, hashed name.
if server.len() + DELIM.len() >= MAX_LEN {
    // continue; // bad: try a hashed display name instead
}
```

- **Assume The Model Requires Human-Readable Full Names:** Don’t avoid hashing if it’s the safest path to correctness.
```rust
// Prefer a stable hashed suffix when needed; readability is secondary to compliance.
let display = bounded_name(server, tool); // safe, deterministic, model-friendly
```