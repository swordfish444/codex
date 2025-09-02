**DOs**
- Bold: Prefer async I/O with `tokio::fs`: Use async directory iteration and reads; return empty on errors.
```rust
use tokio::fs;
use std::path::Path;
use codex_protocol::custom_prompts::CustomPrompt;

pub async fn discover_prompts_in(dir: &Path) -> Vec<CustomPrompt> {
    let mut out = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return out,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !entry.file_type().await.map(|t| t.is_file()).unwrap_or(false) { continue; }
        if !path.extension().and_then(|s| s.to_str()).map(|e| e.eq_ignore_ascii_case("md")).unwrap_or(false) { continue; }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()) else { continue; };
        let content = match fs::read_to_string(&path).await { Ok(s) => s, Err(_) => continue };
        out.push(CustomPrompt { name, path, content });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
```

- Bold: Use `PathBuf` for file paths: Make it easy to open/edit files later.
```rust
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomPrompt {
    pub name: String,
    pub path: PathBuf,
    pub content: String,
}
```

- Bold: Provide a clear default prompts dir: Resolve `$CODEX_HOME/prompts` safely.
```rust
use std::path::PathBuf;

pub fn default_prompts_dir() -> Option<PathBuf> {
    crate::config::find_codex_home().ok().map(|home| home.join("prompts"))
}
```

- Bold: Import protocol types and use them directly: Keep event construction readable.
```rust
use crate::protocol::{Event, EventMsg, ListCustomPromptsResponseEvent, Op};

let event = Event {
    id: sub_id,
    msg: EventMsg::ListCustomPromptsResponse(ListCustomPromptsResponseEvent { custom_prompts }),
};
```

- Bold: Exclude name collisions with built-ins: Filter user prompts against builtin command names.
```rust
use std::collections::HashSet;

let exclude: HashSet<String> = builtins.iter().map(|(n, _)| (*n).to_string()).collect();
prompts.retain(|p| !exclude.contains(&p.name));
prompts.sort_by(|a, b| a.name.cmp(&b.name));
```

- Bold: Name enum variants precisely: Prefer `UserPrompt` over generic names like `Prompt`.
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandItem {
    Builtin(SlashCommand),
    UserPrompt(usize), // index into prompts vec
}
```

- Bold: Sort filtered items efficiently: Sort by score, then by name; compute names only if needed.
```rust
out.sort_by(|a, b| {
    a.2.cmp(&b.2).then_with(|| {
        let an = match a.0 { CommandItem::Builtin(c) => c.command(), CommandItem::UserPrompt(i) => &self.prompts[i].name };
        let bn = match b.0 { CommandItem::Builtin(c) => c.command(), CommandItem::UserPrompt(i) => &self.prompts[i].name };
        an.cmp(bn)
    })
});
```

- Bold: Show helpful descriptions for prompts: Consider first line as the description.
```rust
let desc = self.prompts[i].content.lines().next().unwrap_or("send saved prompt");
GenericDisplayRow { name: format!("/{}", self.prompts[i].name), description: Some(desc.to_string()), /* ... */ }
```

- Bold: Use inline capture in logs/formatting: Favor `{var}` capture where supported.
```rust
let len = ev.custom_prompts.len();
debug!("received {len} custom prompts"); // tracing's inline capture
let cmd = "init";
let s = format!("/{cmd}");               // Rust inline capture in format!
```

- Bold: Make tests async-friendly and concise: Use `#[tokio::test]`; derive `PartialEq` to assert directly.
```rust
#[derive(Debug, PartialEq)]
enum InputResult { Submitted(String), /* ... */ }

#[tokio::test]
async fn discovers_and_sorts_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.md"), "b").unwrap();
    std::fs::write(dir.path().join("a.md"), "a").unwrap();
    let names: Vec<_> = discover_prompts_in(dir.path()).await.into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["a", "b"]);
}
```

**DON’Ts**
- Bold: Don’t offload light I/O to `spawn_blocking`: Use async fs instead of a blocking thread pool.
```rust
// Anti-pattern
let custom_prompts = tokio::task::spawn_blocking(|| discover_prompts_in(&dir)).await.unwrap_or_default();

// Prefer
let custom_prompts = crate::custom_prompts::discover_prompts_in(&dir).await;
```

- Bold: Don’t use `String` for paths: Use `PathBuf` to preserve platform semantics.
```rust
// Avoid
struct CustomPrompt { path: String, /* ... */ }

// Prefer
struct CustomPrompt { path: std::path::PathBuf, /* ... */ }
```

- Bold: Don’t fully qualify types when imports clarify code: Import once, simplify everywhere.
```rust
// Avoid
msg: EventMsg::ListCustomPromptsResponse(crate::protocol::ListCustomPromptsResponseEvent { custom_prompts })

// Prefer
use crate::protocol::ListCustomPromptsResponseEvent;
msg: EventMsg::ListCustomPromptsResponse(ListCustomPromptsResponseEvent { custom_prompts })
```

- Bold: Don’t leak collisions into the UI: Never show user prompts that shadow built-ins.
```rust
let exclude: HashSet<String> = builtins.iter().map(|(n, _)| (*n).to_string()).collect();
prompts.retain(|p| !exclude.contains(&p.name)); // required
```

- Bold: Don’t over-compute during sort: Avoid building names before comparing scores.
```rust
// Avoid: compute names first
out.sort_by(|a, b| {
    let an = /* ... */; let bn = /* ... */;
    a.2.cmp(&b.2).then(an.cmp(bn))
});

// Prefer: compute only inside then_with
out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| /* compute names here */));
```

- Bold: Don’t write verbose assertions: Use one-line `assert_eq!` when the type implements `PartialEq`.
```rust
assert_eq!(InputResult::Submitted(prompt_text.to_string()), result);
```

- Bold: Don’t assume sync tests for async code: Mark async tests with `#[tokio::test]`.
```rust
#[tokio::test]
async fn skips_non_utf8_files() { /* ... */ }
```