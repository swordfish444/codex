**DOs**
- **Use AGENTS.md:** Read user instructions from `$CODEX_HOME/AGENTS.md` only.
```rust
use std::path::PathBuf;

fn load_user_instructions() -> Option<String> {
    let mut p = std::env::var_os("CODEX_HOME").map(PathBuf::from)?;
    p.push("AGENTS.md");
    std::fs::read_to_string(&p).ok().and_then(|s| {
        let s = s.trim();
        if s.is_empty() { None } else { Some(s.to_owned()) }
    })
}
```
- **Ignore empty content:** Treat an empty `AGENTS.md` as no instructions.
```rust
let content = std::fs::read_to_string(&p).ok()?;
let trimmed = content.trim();
let user_instructions = (!trimmed.is_empty()).then(|| trimmed.to_owned());
```
- **Align docs and code:** Update comments and docs to reference `AGENTS.md`.
```rust
/// User-provided instructions from AGENTS.md.
pub user_instructions: Option<String>;
```
- **Centralize the filename:** Use a single constant to avoid drift.
```rust
const USER_INSTRUCTIONS_FILE: &str = "AGENTS.md";

// ...
p.push(USER_INSTRUCTIONS_FILE);
```

**DON'Ts**
- **Don’t support instructions.md:** No fallback or dual support with `instructions.md`.
```rust
// Bad: do not keep legacy fallback
p.push("AGENTS.md");
// if not found...
p.pop();
p.push("instructions.md"); // ❌ remove this
```
- **Don’t read instructions.md anywhere:** Remove direct references to `instructions.md`.
```rust
// Bad: legacy file name
p.push("instructions.md"); // ❌
```
- **Don’t return empty strings:** Avoid treating empty files as valid instructions.
```rust
// Bad: returns Some("") instead of None
let user_instructions = std::fs::read_to_string(&p).ok(); // ❌
```
- **Don’t mismatch code and docs:** Avoid comments implying `instructions.md`.
```rust
// Bad: outdated doc
/// User-provided instructions from instructions.md. // ❌
```