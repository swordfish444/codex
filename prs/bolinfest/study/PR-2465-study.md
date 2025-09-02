**Rust 1.89 Upgrade: DOs & DON'Ts**

**DOs**
- Bold: Upgrade toolchains: Update `rust-toolchain.toml` and CI to 1.89.
```
# codex-rs/rust-toolchain.toml
[toolchain]
channel = "1.89.0"
components = ["clippy", "rustfmt", "rust-src"]

# .github/workflows/*
- uses: dtolnay/rust-toolchain@1.89
```

- Bold: Keep Clippy clean: Prefer let‑chains over nested conditionals.
```rust
// Before
if let Some(output) = url_result {
    if output.status.success() {
        if let Ok(url) = String::from_utf8(output.stdout) {
            git_info.repository_url = Some(url.trim().to_string());
        }
    }
}

// After (let-chains)
if let Some(output) = url_result
    && output.status.success()
    && let Ok(url) = String::from_utf8(output.stdout)
{
    git_info.repository_url = Some(url.trim().to_string());
}
```

- Bold: Combine Option checks with value guards.
```rust
// Before
if let Ok(val) = std::env::var("CODEX_HOME") {
    if !val.is_empty() {
        return PathBuf::from(val).canonicalize();
    }
}

// After
if let Ok(val) = std::env::var("CODEX_HOME") && !val.is_empty() {
    return PathBuf::from(val).canonicalize();
}
```

- Bold: Use `is_some_and` to express predicate checks succinctly.
```rust
if let ParsedCommand::Unknown { cmd } = &commands[0]
    && shlex_split(cmd).is_some_and(|t| t.first().map(|s| s.as_str()) == Some("echo"))
{
    return Some(commands[1..].to_vec());
}
```

- Bold: Prefer `std::slice::from_ref` for single‑element slices.
```rust
// Before
seek_sequence(lines, &[ctx_line.clone()], start, false);
// After
seek_sequence(lines, std::slice::from_ref(ctx_line), start, false);

// In tests
checker.check(exec, &cwd, std::slice::from_ref(&root), &[]);
```

- Bold: Adjust APIs for the 1.89 lifetime lint when needed.
```rust
// Before
pub fn get_frame(&mut self) -> Frame {
// After
pub fn get_frame(&mut self) -> Frame<'_> {
```

- Bold: Keep formatting simple: inline variables in `format!` braces.
```rust
with_context(|| format!("Failed to write file {}", path.display()))
```

- Bold: Run the standard hygiene commands locally.
```bash
# In codex-rs/
just fmt
cargo clippy --tests
```

**DON'Ts**
- Bold: Don’t fight Clippy by nesting `if`/`if let`; use guard chains instead.
```rust
// Don’t do this
if let Some(parent) = path.parent() {
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }
}
// Do this
if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() {
    std::fs::create_dir_all(parent)?;
}
```

- Bold: Don’t disable lints to preserve older patterns; embrace 1.89 idioms.
```rust
// Avoid adding #[allow(clippy::collapsible_if)] just to keep nested conditionals.
```

- Bold: Don’t pass manual single‑element arrays where a reference slice is clearer.
```rust
// Don’t
checker.check(exec, &cwd, &[root.clone()], &[]);
// Do
checker.check(exec, &cwd, std::slice::from_ref(&root), &[]);
```

- Bold: Don’t rely on implicit lifetimes that now trigger the mismatched-lifetime lint.
```rust
// Fix signatures like -> Frame to -> Frame<'_> where applicable.
```