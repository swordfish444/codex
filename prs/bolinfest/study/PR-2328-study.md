**DOs**
- Bold: Use clippy.toml for tests: Rely on allow-expect-in-tests and allow-unwrap-in-tests; delete redundant per-file/test #[allow(...)].
```rust
// tests are allowed to use unwrap()/expect() without local attrs now
#[test]
fn parses() {
    let n = "42".parse::<u32>().unwrap();
    assert_eq!(n, 42);
}
```
- Bold: Prefer #[expect(...)] over #[allow(...)] in non-test code: Make the intent explicit and get warned if the lint stops triggering.
```rust
#[expect(clippy::unwrap_used)]
fn parse_config(s: &str) -> u32 {
    s.parse::<u32>().unwrap()
}
```
- Bold: Scope expectations narrowly: Apply at the smallest item (function/const/impl) that needs it, not module/crate level.
```rust
// Good: function-scoped
#[expect(clippy::expect_used)]
fn flush_stdout() {
    use std::io::Write;
    std::io::stdout().flush().expect("flush stdout after printing");
}
```
- Bold: Keep expect messages meaningful: Explain why failure is a bug or impossible in practice.
```rust
#[expect(clippy::expect_used)]
const THREADS: std::num::NonZeroUsize =
    std::num::NonZeroUsize::new(2).expect("literal 2 is non-zero");
```
- Bold: Refactor when practical: Replace unwrap/expect in production paths with Result/Option handling.
```rust
// Instead of:
let port = std::env::var("PORT").unwrap().parse::<u16>().unwrap();

// Prefer:
let port: u16 = std::env::var("PORT")
    .map_err(|_| anyhow::anyhow!("PORT not set"))?
    .parse()
    .map_err(|e| anyhow::anyhow!("invalid PORT: {e}"))?;
```
- Bold: Cover integration-test helpers explicitly: Helpers not marked #[test] still need targeted #[expect(...)].
```rust
#[expect(clippy::unwrap_used)]
fn assert_role(req: &serde_json::Value, role: &str) {
    assert_eq!(req["role"].as_str().unwrap(), role);
}
```
- Bold: Be surgical with stdout expectations: Only expect print_stdout where printing is intentional (e.g., CLI output).
```rust
#[expect(clippy::print_stdout)]
fn show_status(msg: &str) {
    println!("{msg}");
}
```

**DON’Ts**
- Bold: Don’t blanket-allow at crate/module scope: Avoid #![allow(clippy::unwrap_used)] or #![allow(clippy::expect_used)] that mask real issues.
```rust
// ❌ Avoid
// #![allow(clippy::unwrap_used)]
// #![allow(clippy::expect_used)]
```
- Bold: Don’t stack unrelated lints casually: Expect only what the item actually triggers; split if needed.
```rust
// ❌ Avoid piling on
// #[expect(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

// ✅ Prefer targeted
#[expect(clippy::print_stdout)]
fn print_only() { println!("ok"); }
```
- Bold: Don’t keep #[expect] where it’s removable: If you eliminate the unwrap/expect, remove the attribute too.
```rust
// After refactor, delete the now-stale expectation
// #[expect(clippy::unwrap_used)]
fn parse_ok(s: &str) -> anyhow::Result<u32> {
    Ok(s.parse()?)
}
```
- Bold: Don’t rely on test allowances for non-test code: Production code still needs either real error handling or narrowly scoped #[expect(...)].
```rust
// ❌ This is not a test; clippy will flag it
fn prod() {
    let _ = std::env::var("HOME").unwrap();
}
```
- Bold: Don’t use expect without context: Opaque messages make failures harder to diagnose.
```rust
// ❌ Bad
file.read_to_end(&mut buf).expect("failed");

// ✅ Better
file.read_to_end(&mut buf).expect("read_to_end: config file must be readable");
```