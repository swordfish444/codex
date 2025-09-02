**DOs**
- Enforce read-only `.git` under writable roots: Mark only the top-level `.git/` inside each writable root as read-only; leave the rest writable.
```rust
// In get_writable_roots_with_cwd(...)
let top_level_git = writable_root.join(".git");
if top_level_git.is_dir() {
    subpaths.push(top_level_git);
}
```

- Represent writable roots with read-only subpaths: Use `WritableRoot { root, read_only_subpaths }` instead of plain `PathBuf`.
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot {
    pub root: PathBuf,
    pub read_only_subpaths: Vec<PathBuf>,
}
```

- Canonicalize paths before emitting Seatbelt params: Avoid `/var` vs `/private/var` mismatches on macOS.
```rust
let canonical_root = wr.root.canonicalize().unwrap_or_else(|_| wr.root.clone());
let root_param = format!("WRITABLE_ROOT_{index}");
cli_args.push(format!("-D{root_param}={}", canonical_root.to_string_lossy()));
```

- Generate Seatbelt rules with require-not for protected subpaths: Combine `(subpath ...)` with `(require-not ...)` for each read-only subpath.
```rust
let mut parts = vec![format!("(subpath (param \"{root_param}\"))")];
for (i, ro) in wr.read_only_subpaths.iter().enumerate() {
    let ro_param = format!("WRITABLE_ROOT_{index}_RO_{i}");
    let ro_path = ro.canonicalize().unwrap_or_else(|_| ro.clone());
    cli_args.push(format!("-D{ro_param}={}", ro_path.to_string_lossy()));
    parts.push(format!("(require-not (subpath (param \"{ro_param}\")))"));
}
let policy = format!("(require-all {} )", parts.join(" "));
writable_folder_policies.push(policy);
```

- Include defaults only when requested: Add `cwd` (and `TMPDIR` on macOS) when `include_default_writable_roots` is true.
```rust
if include_default_writable_roots {
    roots.push(cwd.to_path_buf());
    if cfg!(target_os = "macos") {
        if let Some(tmp) = std::env::var_os("TMPDIR") {
            roots.push(PathBuf::from(tmp));
        }
    }
}
```

- Set `CODEX_SANDBOX` when spawning under Seatbelt: Pass env by value and insert the marker before spawning.
```rust
pub async fn spawn_command_under_seatbelt(
    /* ... */, mut env: HashMap<String, String>,
) -> std::io::Result<Child> {
    env.insert(CODEX_SANDBOX_ENV_VAR.to_string(), "seatbelt".to_string());
    // spawn_child_async(..., env)
}
```

- Test both explicit roots and default roots: Verify `.git` is read-only when present, and TMPDIR handling on macOS.
```rust
let args = create_seatbelt_command_args(cmd.clone(), &policy, cwd);
let expected = format!("{base}\n(allow file-read*)\n(allow file-write*\n(require-all (subpath (param \"WRITABLE_ROOT_0\")) (require-not (subpath (param \"WRITABLE_ROOT_0_RO_0\"))) )\n)\n", base = MACOS_SEATBELT_BASE_POLICY);
assert_eq!(args[0], "-p");
assert_eq!(args[1], expected);
```

- Add integration tests that skip under Seatbelt: Don’t run Seatbelt-in-Seatbelt; skip when `CODEX_SANDBOX=seatbelt`.
```rust
if std::env::var(CODEX_SANDBOX_ENV_VAR) == Ok("seatbelt".to_string()) {
    eprintln!("{CODEX_SANDBOX_ENV_VAR} is set to 'seatbelt', skipping test.");
    return;
}
```

- Keep Linux Landlock in sync with the new API: Convert `WritableRoot` to `PathBuf` for now; enforce subpaths later.
```rust
let writable_roots: Vec<PathBuf> = sandbox_policy
    .get_writable_roots_with_cwd(cwd)
    .into_iter()
    .map(|wr| wr.root)
    .collect();
```

- Document the user-facing behavior: Note that with workspace-write on macOS, `.git/` becomes read-only; commands like `git commit` will fail unless explicitly permitted.
```toml
# config excerpt
sandbox_mode = "workspace-write"  # .git/ under writable roots is read-only
```

**DON’Ts**
- Don’t block nested `.git/` in subdirectories: Only protect the top-level `.git/` that is an immediate child of the writable root.
```rust
// Correct: only checks root.join(".git")
let top_level = root.join(".git");
if top_level.is_dir() { /* protect only this */ }
```

- Don’t forget to canonicalize before emitting `-D` args: Policy matching may break without it.
```rust
let p = path.canonicalize().unwrap_or_else(|_| path.clone());
cli_args.push(format!("-D{param}={}", p.to_string_lossy()));
```

- Don’t change the Seatbelt spawn signature to `&mut` env: Passing `env` by value and mutating locally is fine.
```rust
pub async fn spawn_command_under_seatbelt(
    /* ... */, mut env: HashMap<String, String>,
) -> std::io::Result<Child> { /* ... */ }
```

- Don’t run Seatbelt tests when already sandboxed: Skip when `CODEX_SANDBOX=seatbelt` to avoid false negatives.
```rust
if std::env::var(CODEX_SANDBOX_ENV_VAR) == Ok("seatbelt".to_string()) { return; }
```

- Don’t assume Linux enforces read-only subpaths yet: Landlock currently receives only roots; subpath protection is a follow-up.
```rust
// TODO: enforce read_only_subpaths for Landlock in future PR
```

- Don’t treat `.git` file indirections as handled: The `gitdir:` file case is not covered yet; only `.git/` directories are protected.
```text
.git (file with "gitdir: /path") — not yet protected by this PR
```