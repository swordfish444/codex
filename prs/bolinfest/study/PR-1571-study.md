**DOs**
- **Use Deterministic Qualification:** Prefer the simplest stable name; only add context when needed.
```rust
use sha1::{Digest, Sha1};
const MAX: usize = 64;
const DELIM: &str = "__";

fn qualify_name(server: &str, tool: &str, used: &mut std::collections::HashSet<String>) -> String {
    // 1) tool
    if tool.len() <= MAX && used.insert(tool.to_string()) {
        return tool.to_string();
    }
    // 2) server__tool
    let st = format!("{server}{DELIM}{tool}");
    if st.len() <= MAX && used.insert(st.clone()) {
        return st;
    }
    // 3) prefix + hash(server__tool), still <= MAX
    let mut hasher = Sha1::new();
    hasher.update(st.as_bytes());
    let hash = format!("{:x}", hasher.finalize()); // 40 hex chars
    let extra = 0; // or 1 if you want an extra '_' between prefix and hash
    let keep = MAX.saturating_sub(hash.len() + extra);
    let prefix: String = tool.chars().take(keep).collect();
    let qualified = if extra == 1 {
        format!("{prefix}_{hash}")
    } else {
        format!("{prefix}{hash}")
    };
    used.insert(qualified.clone());
    qualified
}
```

- **Keep a Canonical Map:** Store `ToolInfo` by qualified name; use it for lookups (don’t re-parse).
```rust
#[derive(Clone)]
struct ToolInfo { server: String, tool: String, /* Tool object, etc. */ }

struct McpConnectionManager {
    tools: std::collections::HashMap<String, ToolInfo>, // qualified -> info
}

impl McpConnectionManager {
    fn parse_tool_name(&self, name: &str) -> Option<(String, String)> {
        self.tools.get(name).map(|t| (t.server.clone(), t.tool.clone()))
    }
}
```

- **Cap Names and Use Allowed Delimiters:** Enforce 64-char max and stick to `a-zA-Z0-9_-` (e.g., `"__"`).
```rust
const MAX_TOOL_NAME_LENGTH: usize = 64;
const MCP_TOOL_NAME_DELIMITER: &str = "__";

fn is_valid_mcp_server_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
```

- **Warn-And-Skip Duplicates:** Deduplicate consistently; log and continue instead of panicking.
```rust
use tracing::warn;
let mut used = std::collections::HashSet::new();
let name = qualify_name(&server, &tool, &mut used);
if !used.insert(name.clone()) {
    warn!("skipping duplicated tool {}", name);
    // continue;
}
```

- **Stabilize Ordering in UX/Tests:** Don’t rely on async join order or `HashMap` iteration; sort keys when needed.
```rust
let mut keys: Vec<_> = manager.tools.keys().cloned().collect();
keys.sort(); // stable output / assertions
assert!(!keys.is_empty());
```

- **Test The Edge Cases:** Cover unique, duplicate, and long-name scenarios with focused unit tests.
```rust
#[test]
fn qualify_unique_and_long_names() {
    let mut used = std::collections::HashSet::new();
    let a = qualify_name("s1", "tool", &mut used);
    let b = qualify_name("s2", "tool", &mut used); // forces fallback naming
    assert_ne!(a, b);
    assert!(a.len() <= 64 && b.len() <= 64);
}
```

**DON’Ts**
- **Don’t Add Unrelated Changes:** Keep the PR focused on MCP tool naming; avoid drive-by edits elsewhere.
```diff
- // codex-cli/bin/codex.js (unrelated)
- const wantsNative = fs.existsSync(path.join(__dirname, "use-native")) ||
+ // Remove unrelated change from this PR
```

- **Don’t Use Random Suffixes:** Avoid per-run randomness; names must be stable across rollouts.
```rust
// Bad: non-deterministic
let suffix = format!("{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
let name = format!("{}__{}_{suffix}", server, tool);
```

- **Don’t Re-Parse Strings:** Stop splitting on delimiters to recover server/tool; look up the map instead.
```rust
// Bad
let (server, tool) = name.split_once("__").unwrap();

// Good
let (server, tool) = manager.parse_tool_name(name).expect("unknown tool name");
```

- **Don’t Rely On Insertion/Join Order:** Async listing order is not stable; never assert on raw map iteration.
```rust
// Bad: order-dependent UI/test
for (k, _) in &manager.tools { println!("{k}"); } // nondeterministic
```

- **Don’t Exceed Limits Or Use Invalid Delimiters:** Respect 64-char max and allowed charset; avoid exotic separators.
```rust
// Bad: too long, invalid delimiter
let name = format!("{server}||{}", tool); // '||' not allowed, length unchecked
```

- **Don’t Panic On Collisions:** Collisions happen; handle gracefully with deterministic dedup and logging.
```rust
// Bad
panic!("tool name collision for '{}'", name);
```