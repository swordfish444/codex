**DOs**
- **Pin dev-dependencies:** Use exact patch versions to avoid drift and CI surprises.
```toml
# Good
[dev-dependencies]
libc = "0.2.172"
```

- **Keep test focus at top:** Put small consts and the actual tests first; move heavy helpers below the tests.
```rust
// tests/sandbox.rs

const IN_SANDBOX_ENV_VAR: &str = "IN_SANDBOX";

#[tokio::test]
async fn allow_unix_socketpair_recvfrom() {
    run_code_under_sandbox(
        "allow_unix_socketpair_recvfrom",
        &SandboxPolicy::ReadOnly,
        || async { unix_sock_body() },
    )
    .await
    .expect("reexec succeeded");
}

// --- Helpers below ---

fn unix_sock_body() {
    unsafe {
        // AF_UNIX socketpair + recvfrom + cleanup
    }
}

pub async fn run_code_under_sandbox<F, Fut>(/* ... */) -> io::Result<Option<ExitStatus>> {
    // helper impl
}
```

- **Isolate unsafe code:** Put all unsafe ops in a dedicated function to reduce nesting and localize unsafety.
```rust
fn unix_sock_body() {
    unsafe {
        let mut fds = [0; 2];
        assert_eq!(libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr()), 0);

        let msg = b"hello_unix";
        assert!(libc::write(fds[0], msg.as_ptr().cast(), msg.len()) >= 0);

        let mut buf = [0u8; 64];
        let n = libc::recvfrom(fds[1], buf.as_mut_ptr().cast(), buf.len(), 0, std::ptr::null_mut(), std::ptr::null_mut());
        assert!(n >= 0);
        assert_eq!(&buf[..n as usize], msg);

        let _ = libc::close(fds[0]);
        let _ = libc::close(fds[1]);
    }
}

#[tokio::test]
async fn allow_unix_socketpair_recvfrom() {
    run_code_under_sandbox("allow_unix_socketpair_recvfrom", &SandboxPolicy::ReadOnly, || async {
        unix_sock_body()
    })
    .await
    .expect("reexec succeeded");
}
```

- **Edit Cargo.toml manually when needed:** `cargo add` may ignore repo conventions; adjust versions by hand.
```toml
# Manually keep versions pinned
[dev-dependencies]
libc = "0.2.172"
```

**DON’Ts**
- **Don’t use broad semver or pre-releases:** Avoid ranges and alpha tags that can pull unintended versions.
```toml
# Avoid
[dev-dependencies]
libc = "0.2"                  # too broad
# libc = "0.2.172-alpha.1"    # pre-release
```

- **Don’t put heavy helpers at the top:** Large re-exec/launcher helpers should not bury the tests.
```rust
// Avoid: long helper first, tests later
pub async fn run_code_under_sandbox<F, Fut>(/* ... */) -> io::Result<Option<ExitStatus>> {
    // long helper impl...
}

#[tokio::test]
async fn allow_unix_socketpair_recvfrom() { /* test now buried */ }
```

- **Don’t embed big unsafe blocks in test closures:** Extract them to a function to avoid 2–3 extra indentation levels.
```rust
// Avoid
#[tokio::test]
async fn allow_unix_socketpair_recvfrom() {
    run_code_under_sandbox("allow_unix_socketpair_recvfrom", &SandboxPolicy::ReadOnly, || async {
        unsafe {
            // long AF_UNIX + recvfrom body deeply nested here
        }
    })
    .await
    .unwrap();
}
```