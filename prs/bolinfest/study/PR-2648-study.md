**DOs**
- **End fixtures with a trailing newline:** Prefer POSIX-style EOF newlines in JSON/text fixtures when they don’t change semantics; it avoids brittle EOF-sensitive checks and matches common tooling expectations. ([github.com](https://github.com/openai/codex/pull/2648/commits/cc27f7529341e722902159d9e6aebaa2273ca41a))
```diff
*** Begin Patch
*** Update File: codex-rs/exec/tests/fixtures/sse_response_completed.json
@@
-]
+]
+
*** End Patch
```

- **Be explicit about reliability in PR descriptions:** When refactoring tests, list the concrete changes that improve stability (e.g., shared Wiremock helper, deterministic response sequencing, consistent runtime settings) rather than saying “clean up.” ([github.com](https://github.com/openai/codex/pull/2648))
```markdown
## Summary
Refactor apply_patch E2E tests for clarity and stability.

## Why this is more reliable
- Use shared `run_e2e_exec_test` to sequence responses deterministically.
- Replace inline SSE with fixtures to reduce duplication.
- Align Tokio runtime settings across tests.
```

- **Normalize only what’s incidental in assertions:** If whitespace/newlines are not what you’re testing, compare normalized strings to prevent accidental failures while still asserting the real behavior. ([github.com](https://github.com/openai/codex/pull/2648/commits/cc27f7529341e722902159d9e6aebaa2273ca41a))
```rust
let expected = include_str!("../fixtures/sse_response_completed.json");
let actual = std::fs::read_to_string(path)?;
assert_eq!(
  actual.trim_end_matches('\n'),
  expected.trim_end_matches('\n')
);
```

- **Address review feedback with focused commits:** When a reviewer flags a safe formatting improvement (like EOF newlines), follow up with a small, targeted commit that does exactly that. ([github.com](https://github.com/openai/codex/pull/2648/commits/cc27f7529341e722902159d9e6aebaa2273ca41a))
```text
git commit -m "tests(exec): add trailing newlines to SSE fixtures"
```

**DON’Ts**
- **Don’t rely on “no newline at EOF” behavior:** Tests shouldn’t assume the last byte isn’t a newline; they’ll break once a formatter or editor adds it. ([github.com](https://github.com/openai/codex/pull/2648/commits/cc27f7529341e722902159d9e6aebaa2273ca41a))
```rust
// Brittle: fails if a trailing newline is later added.
let s = std::fs::read_to_string(".../sse_response_completed.json")?;
assert!(s.ends_with(']')); // Avoid EOF-structure assumptions
```

- **Don’t ship vague PR rationales:** “Clean up tests” without stating the specific reliability benefits forces reviewers to guess what changed and why it’s safer. Be concrete. ([github.com](https://github.com/openai/codex/pull/2648))
```markdown
# Bad
Refactor tests. Clean up and make more solid.

# Good
Extract Wiremock sequencing helper; move SSE to fixtures; align runtime settings for determinism.
```

- **Don’t conflate formatting with semantics in test expectations:** If the behavior under test is “final file content is X,” don’t make the assertion fail on incidental whitespace differences unless that’s intentional. ([github.com](https://github.com/openai/codex/pull/2648))
```rust
// Prefer checking the meaningful content:
assert_eq!(actual.trim_end_matches('\n'), "Final text");
```