**DOs**
- **Use `include_default_writable_roots` (bool):** Add this flag to `SandboxPolicy::WorkspaceWrite` to control whether defaults like `cwd` and `TMPDIR` are writable. Defaults to `true` via serde.
```rust
#[derive(serde::Deserialize, serde::Serialize)]
pub enum SandboxPolicy {
    WorkspaceWrite {
        writable_roots: Vec<PathBuf>,
        #[serde(default)]
        network_access: bool,
        #[serde(default = "default_true")]
        include_default_writable_roots: bool,
    },
}

fn default_true() -> bool { true }
```

- **Keep default behavior explicit in constructors:** For normal operation, set `include_default_writable_roots: true` (or rely on the serde default).
```rust
let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false,
    include_default_writable_roots: true,
};
```

- **Use `include_default_writable_roots: false` for strict tests:** When you need to verify that only specified paths are writable (no `cwd`, no `TMPDIR`), disable defaults.
```rust
let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![PathBuf::from("/tmp/test-writable")],
    network_access: false,
    include_default_writable_roots: false,
};

let roots = policy.get_writable_roots_with_cwd(Path::new("/workspace"));
assert_eq!(roots, vec![PathBuf::from("/tmp/test-writable")]);
```

- **Reflect strictness in summaries:** Append “(exact writable roots)” to the summary when defaults are excluded.
```rust
let p = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false,
    include_default_writable_roots: false,
};
assert_eq!(summarize_sandbox_policy(&p), "workspace-write (exact writable roots)");
```

- **Map config → policy with defaults preserved:** When deriving from config, propagate `network_access` and `writable_roots`, and set `include_default_writable_roots: true` for the common path.
```rust
// From ConfigToml → SandboxPolicy
SandboxPolicy::WorkspaceWrite {
    writable_roots: s.writable_roots.clone(),
    network_access: s.network_access,
    include_default_writable_roots: true,
}
```

- **Follow existing tests’ intent:** For integration tests that operate via `TMPDIR` or need normal behavior, leave `include_default_writable_roots: true`.
```rust
let sandbox_policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: writable_roots.to_vec(),
    network_access: false,
    include_default_writable_roots: true,
};
```

**DON’Ts**
- **Don’t introduce “negative” naming here:** Avoid flags like `use_exact_writable_roots` that read awkwardly with `false`; prefer `include_default_writable_roots` with a `true` default.
```rust
// Avoid
// include: use_exact_writable_roots: false
```

- **Don’t assume defaults in strict tests:** If a test asserts non-writability outside specific paths, do not rely on the default behavior—set `include_default_writable_roots: false`.
```rust
// Wrong for strict tests: this silently makes cwd/TMPDIR writable
let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![PathBuf::from("/only/this")],
    network_access: false,
    include_default_writable_roots: true, // ← will add defaults
};
```

- **Don’t forget serde defaults:** If you add the field to serialized configs or protocol, ensure `#[serde(default = "default_true")]` is present so older configs don’t break.
```rust
// Missing serde default would make deserialization fail or flip semantics.
#[serde(default = "default_true")]
include_default_writable_roots: bool,
```

- **Don’t hide strict mode in summaries:** When `include_default_writable_roots` is `false`, make sure summaries indicate it with “(exact writable roots)”.
```rust
// In summary rendering: add the note when defaults are excluded.
if !include_default_writable_roots { summary.push_str(" (exact writable roots)"); }
```