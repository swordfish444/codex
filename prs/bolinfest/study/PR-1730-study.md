**PR #1730 Review Takeaways (bolinfest)**

**DOs**
- Bold the keyword: concise description.
- Provide short code blocks that show the pattern.

**DOs**
- Use async file I/O with Tokio: prefer `tokio::fs` + `BufReader` and limit reads.
```rust
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt, BufReader};

async fn read_up_to(path: &std::path::Path, max_bytes: usize) -> io::Result<Option<String>> {
    let file = match File::open(path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    reader.take(max_bytes as u64).read_to_end(&mut buf).await?;
    let s = String::from_utf8_lossy(&buf).to_string();
    Ok((!s.trim().is_empty()).then_some(s))
}
```

- Make path discovery async and stop at the Git root.
```rust
use tokio::fs;
use tokio::io;
use std::path::{Path, PathBuf};

async fn discover_project_doc_path_from_dir(start_dir: &Path, names: &[&str]) -> io::Result<Option<PathBuf>> {
    let mut dir = fs::canonicalize(start_dir).await.unwrap_or_else(|_| start_dir.to_path_buf());

    // Try in the current directory first
    if let Some(p) = first_nonempty_candidate_async(&dir, names).await? {
        return Ok(Some(p));
    }

    // Walk up until a .git is found; do not walk past it
    while let Some(parent) = dir.parent() {
        let git_marker = dir.join(".git");
        if fs::metadata(&git_marker).await.is_ok() {
            return first_nonempty_candidate_async(&dir, names).await;
        }
        dir = parent.to_path_buf();
    }
    Ok(None)
}
```

- Consolidate discovery logic; reuse it for both “path” and “contents” to avoid duplication.
```rust
async fn first_nonempty_candidate_async(dir: &Path, names: &[&str]) -> io::Result<Option<PathBuf>> {
    for name in names {
        let candidate = dir.join(name);
        if let Some(_) = read_up_to(&candidate, 8192).await? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

async fn find_project_doc(config: &Config) -> io::Result<Option<String>> {
    let Some(path) = discover_project_doc_path_from_dir(&config.cwd, CANDIDATE_FILENAMES).await? else {
        return Ok(None);
    };
    read_up_to(&path, config.project_doc_max_bytes).await
}
```

- Return early using expression style; list the common case first.
```rust
pub async fn discover_project_doc_path(config: &Config) -> io::Result<Option<PathBuf>> {
    if config.project_doc_max_bytes > 0 {
        discover_project_doc_path_from_dir(&config.cwd, CANDIDATE_FILENAMES).await
    } else {
        Ok(None)
    }
}
```

- Pass `agents_doc_path` in `SessionConfiguredEvent`; let the TUI just render it.
```rust
// core: when sending the event
EventMsg::SessionConfigured(SessionConfiguredEvent {
    session_id,
    model,
    agents_doc_path: agents_doc_path_string(&config),
    history_log_id,
    history_entry_count,
});

// tui: consume what core sent (no filesystem work here)
let model = event.model.clone();
let agents_doc_path = event.agents_doc_path.clone();
```

- Canonicalize early; comment accurately (“start_dir contains ..”).
```rust
// Avoid loops if start_dir contains `..`
let dir = fs::canonicalize(start_dir).await.unwrap_or_else(|_| start_dir.to_path_buf());
```

- Treat whitespace-only files as empty; enforce byte limits.
```rust
let s = String::from_utf8_lossy(&buf);
if s.trim().is_empty() { return Ok(None); } // consider as not found
```

- Inline variables in `format!`.
```rust
let u = "/home/me/.codex/AGENTS.md";
let pr = "/repo/AGENTS.md";
let summary = format!("Using user instructions ({u}) and project instructions ({pr})");
```

- Cover precedence and disabling in tests (cwd beats repo root; repo root fallback; 0-byte limit disables).
```rust
assert_eq!(discover_project_doc_path(&cfg).await?.unwrap(), nested.join("AGENTS.md"));
```


**DON’Ts**
- Use blocking `std::fs`/`std::io::Read` in async code paths; prefer Tokio equivalents.
```rust
// ❌ Avoid in async contexts:
use std::fs::File;
use std::io::Read;
```

- Assume `Read::read()` fills the buffer; respect the returned byte count or use `read_to_end()` with `take()`.
```rust
// ❌ Anti-pattern:
let n = file.read(&mut buf)?; // may be < buf.len()
// ✅ Use AsyncReadExt::take + read_to_end, or handle `n` carefully
```

- Duplicate discovery and loading logic across functions.
```rust
// ❌ Don’t reimplement “find candidate, then read” in multiple places
// ✅ Centralize in `first_nonempty_candidate_async` and reuse
```

- Do filesystem discovery in the TUI at startup; don’t block the UI thread.
```rust
// ❌ TUI calling filesystem discovery directly
// ✅ Have core include `agents_doc_path` in the session-configured event
```

- Walk past the Git root or mis-detect it.
```rust
// ❌ Continuing upwards after finding `.git`
// ✅ Stop once `.git` exists in the directory
```

- Bury imports or helpers mid-file; keep `use` and helpers at the top for clarity.
```rust
// ✅ Top-of-file imports
use tokio::fs;
use tokio::io::{self, AsyncReadExt, BufReader};
```