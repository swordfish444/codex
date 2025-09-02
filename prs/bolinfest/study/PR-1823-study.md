**DOs**
- **Prefer behavior-based tests**: Validate real effects under Seatbelt instead of checking policy text.
```rust
#[cfg(target_os = "macos")]
#[tokio::test]
async fn python_lock_works_under_seatbelt() {
    use super::{spawn_command_under_seatbelt, SandboxPolicy};
    use crate::spawn::StdioPolicy;
    use std::collections::HashMap;

    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        network_access: false,
        include_default_writable_roots: true,
    };

    let py = r#"import multiprocessing as mp
def f(l):
    with l: pass
if __name__ == "__main__":
    l = mp.Lock()
    p = mp.Process(target=f, args=(l,))
    p.start(); p.join()
"#;

    let mut child = spawn_command_under_seatbelt(
        vec!["python3".into(), "-c".into(), py.into()],
        &policy,
        std::env::current_dir().unwrap(),
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    ).await.expect("spawn under seatbelt");

    let status = child.wait().await.expect("wait for child");
    assert!(status.success(), "python exited with {status:?}");
}
```

- **Gate macOS-only logic**: Use platform guards so Seatbelt tests don’t run where unsupported.
```rust
#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_specific_behavior() {
    // macOS-only assertions here
}
```

- **Use the Seatbelt helper**: Rely on `spawn_command_under_seatbelt` rather than rolling your own sandboxing.
```rust
use super::{spawn_command_under_seatbelt, SandboxPolicy};
use crate::spawn::StdioPolicy;

let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false,
    include_default_writable_roots: true,
};

let mut child = spawn_command_under_seatbelt(
    vec!["/usr/bin/true".into()],
    &policy,
    std::env::current_dir().unwrap(),
    StdioPolicy::RedirectForShellTool,
    std::collections::HashMap::new(),
).await?;
```

- **Assert with context**: Provide clear failure messages with inline variables for quick diagnosis.
```rust
let status = child.wait().await?;
assert!(status.success(), "process failed: {status:?}");
```

- **Keep tests hermetic**: Avoid network and mutable global state; configure the policy to be restrictive.
```rust
let policy = SandboxPolicy::WorkspaceWrite {
    writable_roots: vec![],
    network_access: false, // hermetic
    include_default_writable_roots: true,
};
```

**DON’Ts**
- **Don’t test policy strings directly**: These are brittle and duplicate behavior tests.
```rust
#[test]
fn bad_string_based_policy_test() {
    // Fragile: asserts on implementation detail, not behavior
    assert!(MACOS_SEATBELT_BASE_POLICY.contains("(allow ipc-posix-sem)"));
}
```

- **Don’t run Seatbelt tests cross‑platform**: Missing guards will cause spurious failures on non‑macOS.
```rust
// Bad: no #[cfg(target_os = "macos")]
#[tokio::test]
async fn runs_everywhere_but_shouldnt() {
    // This may fail on Linux/Windows
}
```

- **Don’t bypass the helper or shell out to sandbox tools directly**: Centralize sandbox behavior via the core API.
```rust
// Bad: manual sandbox invocation
use std::process::Command;

#[test]
fn bad_manual_sandbox_exec() {
    let _ = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", "…", "python3", "-c", "print('ok')"])
        .status()
        .unwrap();
}
```

- **Don’t use vague assertions**: Lack of context slows triage.
```rust
// Bad: no context on failure
assert!(status.success());
```

- **Don’t depend on network access in sandboxed tests**: Seatbelt and CI may block it, making tests flaky.
```rust
// Bad: external network dependency
#[tokio::test]
async fn bad_network_test() {
    let _ = reqwest::get("https://example.com").await.unwrap(); // may fail under sandbox
}
```