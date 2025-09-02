**PR #2340 Review — Practical Guide**

**DOs**
- Use `MutexExt::lock_unchecked`: Replace `.lock().unwrap()` when poisoning is unrecoverable; centralize the panic message.
```rust
use std::sync::{Mutex, MutexGuard};

trait MutexExt<T> {
    fn lock_unchecked(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unchecked(&self) -> MutexGuard<'_, T> {
        #[expect(clippy::expect_used)]
        self.lock().expect("poisoned lock")
    }
}

// Usage
let mut state = self.state.lock_unchecked();
```
- Scope the extension locally: Define the trait privately in the module that needs it; add elsewhere only as needed.
```rust
// No `pub` on the trait or impl; file-local by default.
trait MutexExt<T> { /* ... */ }
```
- Prefer `let`-chains over `unwrap` on `Option`: Keep branches safe and clear.
```rust
if sess.show_raw_agent_reasoning && let Some(content) = content {
    for item in content {
        // ...
    }
}
```
- Drive patch safety from `SandboxPolicy`: Pass the policy (not ad‑hoc roots) to safety checks.
```rust
use crate::safety::{assess_patch_safety, SafetyCheck};

match assess_patch_safety(
    &action,
    sess.get_approval_policy(),
    sess.get_sandbox_policy(),
    sess.get_cwd(),
) {
    SafetyCheck::AutoApprove { .. } => { /* run apply_patch in sandbox */ }
    SafetyCheck::NeedsApproval { .. } => { /* request approval */ }
}
```
- Use `WritableRoot::is_path_writable` to respect read‑only subpaths.
```rust
use crate::protocol::WritableRoot;

let root = WritableRoot {
    root: cwd.clone(),
    read_only_subpaths: vec![cwd.join(".git")],
};

assert!(root.is_path_writable(&cwd.join("src/lib.rs")));
assert!(!root.is_path_writable(&cwd.join(".git/HEAD")));
```
- Write robust tests with `TempDir` and explicit policy that excludes temp dirs by default.
```rust
use tempfile::TempDir;
use crate::protocol::SandboxPolicy;

let tmp = TempDir::new().unwrap();
let cwd = tmp.path().to_path_buf();

let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false,
    exclude_tmpdir_env_var: true,
    exclude_slash_tmp: true,
};

// Inside workspace → allowed; outside → denied unless added explicitly.
```
- Localize lint allowances: Remove crate‑wide `unwrap` allowances; annotate narrowly where justified.
```rust
#[expect(clippy::expect_used)]
self.lock().expect("poisoned lock");
```
- Consider `RwLock` for read‑heavy paths: Audit access patterns; use `Mutex` only when appropriate.
```rust
use std::sync::RwLock;
// If mostly reads:
let cache = RwLock::new(MyCache::default());
```

**DON’Ts**
- Don’t call `.lock().unwrap()` on `Mutex`.
```rust
// Bad
let mut st = self.state.lock().unwrap();
// Good
let mut st = self.state.lock_unchecked();
```
- Don’t reintroduce crate‑wide `#![expect(clippy::unwrap_used)]`.
```rust
// Bad (crate root)
#![expect(clippy::unwrap_used)]
```
- Don’t pass `Vec<PathBuf>` “writable roots” to safety checks anymore.
```rust
// Bad
assess_patch_safety(&action, policy, &writable_roots, &cwd);
// Good
assess_patch_safety(&action, policy, sess.get_sandbox_policy(), &cwd);
```
- Don’t assume system temp dirs are writable in tests under `WorkspaceWrite`.
```rust
// Bad: leaves temp dirs implicitly writable
let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false,
    exclude_tmpdir_env_var: false,
    exclude_slash_tmp: false,
};
```
- Don’t `unwrap()` `Option` inside conditionals; pattern‑match instead.
```rust
// Bad
if flag && content.is_some() { for x in content.unwrap() { /* ... */ } }
// Good
if flag && let Some(content) = content { /* ... */ }
```
- Don’t globalize the lock extension trait unless multiple modules need it.
```rust
// Bad
pub trait MutexExt<T> { /* ... */ }  // exported crate‑wide without need
```
- Don’t expect writes to be allowed under `ReadOnly` policy; treat `DangerFullAccess` as unconstrained and `WorkspaceWrite` as scoped.
```rust
// ReadOnly → no writes; DangerFullAccess → unconstrained; WorkspaceWrite → configured roots + cwd
```