**DOs**
- Bold: Document the shim: Explain why `applypatch` is accepted, link a tracking issue, and define removal criteria.
```rust
// Back-compat shim for legacy command "applypatch".
// Context: Some models still emit "applypatch" instead of "apply_patch".
// Tracking: https://github.com/openai/codex/issues/12345
// Removal criteria: remove after <0.1% usage for 30 consecutive days.
const APPLY_PATCH_COMMANDS: [&str; 2] = ["apply_patch", "applypatch"];
```
- Bold: Use an explicit allowlist: Match commands via a small, static list instead of ad hoc checks.
```rust
pub fn maybe_parse_apply_patch(argv: &[String]) -> MaybeApplyPatch {
    match argv {
        [cmd, body] if APPLY_PATCH_COMMANDS.contains(&cmd.as_str()) => match parse_patch(body) {
            Ok(source) => MaybeApplyPatch::Body(source),
            Err(e) => MaybeApplyPatch::PatchParseError(e),
        },
        _ => MaybeApplyPatch::NotApplyPatch(argv.to_vec()),
    }
}
```
- Bold: Add targeted tests: Cover the new alias without changing existing semantics.
```rust
#[test]
fn accepts_applypatch() {
    let args = vec![
        "applypatch".to_string(),
        "*** Begin Patch\n*** Add File: foo\n+hi\n*** End Patch\n".to_string(),
    ];
    match maybe_parse_apply_patch(&args) {
        MaybeApplyPatch::Body(ApplyPatchArgs { hunks, .. }) => assert_eq!(
            hunks,
            vec![Hunk::AddFile { path: "foo".into(), contents: "hi\n".to_string() }]
        ),
        other => panic!("expected Body, got {other:?}"),
    }
}
```
- Bold: Keep behavior identical: Only add the alias; don’t alter parsing, errors, or return types.
```rust
// New alias is routed through the same parse path; no special cases.
let result = parse_patch(body);
```
- Bold: Record a removal plan: Assign owner/date and expected signal to check before removing.
```rust
// TODO(owner: @alice, by: 2025-11-01):
// Check telemetry; if "applypatch" <0.1% for 30d, delete alias + tests.
```

**DON’Ts**
- Bold: Don’t pattern-match loosely: Avoid `starts_with`, regexes, or broad wildcards.
```rust
// ✘ Too broad: could accept unintended commands.
if cmd.starts_with("apply") { /* ... */ }
```
- Bold: Don’t expand aliases casually: Add only evidenced aliases; keep the list minimal.
```rust
// ✘ Don’t add speculative variants.
const APPLY_PATCH_COMMANDS: [&str; 4] = ["apply_patch", "applypatch", "applyPatch", "apply-Patch"];
```
- Bold: Don’t skip rationale/comments: Merging silent shims without context makes future cleanup risky.
```rust
// ✘ Missing why/when to remove; add comment + issue link.
const APPLY_PATCH_COMMANDS: [&str; 2] = ["apply_patch", "applypatch"];
```
- Bold: Don’t duplicate parsing logic: Reuse `parse_patch` to keep behavior consistent.
```rust
// ✘ Don’t fork logic for "applypatch".
if cmd == "applypatch" { /* custom parser */ } // avoid
```
- Bold: Don’t change user-facing behavior: No warnings/errors/log spam; the alias should be silent and backwards-compatible.
```rust
// ✘ Avoid noisy logs for expected alias usage.
eprintln!("Warning: deprecated command 'applypatch'"); // not needed
```