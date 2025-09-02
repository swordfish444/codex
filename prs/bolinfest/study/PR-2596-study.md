**DOs**
- Multiple context lines: Parse all leading `@@` lines into `chunk.context_lines` and advance `start_index` accordingly.
```rust
// parser.rs
let mut context_lines = Vec::new();
let mut start_index = 0;
while start_index < lines.len() {
    if let Some(ctx) = lines[start_index].strip_prefix(CHANGE_CONTEXT_MARKER) {
        context_lines.push(ctx.to_string());
        start_index += 1;
    } else if lines[start_index] == EMPTY_CHANGE_CONTEXT_MARKER {
        start_index += 1;
    } else {
        break;
    }
}
```

- Sequential seeking: Find each context line in order and advance `line_index` after every match; fail with an informative error.
```rust
// lib.rs
for (i, ctx) in chunk.context_lines.iter().enumerate() {
    if let Some(idx) = seek_sequence::seek_sequence(
        original_lines,
        std::slice::from_ref(ctx),
        line_index,
        false,
    ) {
        line_index = idx + 1;
    } else {
        return Err(ApplyPatchError::ComputeReplacements(
            format!("Failed to find context {}/{}: '{}' in {}",
                    i + 1, chunk.context_lines.len(), ctx, path.display())
        ));
    }
}
```

- Context‑aware insertion: For pure additions, insert after the last matched context when not end‑of‑file; otherwise append (respect trailing blank line).
```rust
let insertion_idx = if !chunk.is_end_of_file && !chunk.context_lines.is_empty() {
    line_index
} else if original_lines.last().is_some_and(|s| s.is_empty()) {
    original_lines.len() - 1
} else {
    original_lines.len()
};
```

- Chunk order: Treat the first context line of each chunk as the ordering anchor that must appear after the previous chunk.
```rust
/// Chunks are in file order: the first context line of
/// each chunk must occur later in the file than the previous chunk.
```

- Precise errors: Include which context failed (index/total), the text, and the path; inline variables with `format!`.
```rust
return Err(ApplyPatchError::ComputeReplacements(
    format!("Failed to find context {}/{}: '{}' in {}",
            i + 1, total, ctx_line, path.display())
));
```

- Accurate line numbers: When reporting parser errors after consuming context markers, offset by `start_index`.
```rust
return Err(InvalidHunkError {
    message: "Update hunk does not contain any lines".to_string(),
    line_number: line_number + start_index,
});
```

- Tests for anchors: Cover single and multi‑context insertion to ensure additions land immediately after the final context.
```rust
assert_eq!(
    contents,
    "class BaseClass:\n  def method():\nINSERTED\nline1\n"
);
```

- Newline at EOF: End JSON fixtures (and other text files) with a trailing newline.
```json
[
  { "type": "response.output_item.done", "item": { "type": "custom_tool_call", "name": "apply_patch", "input": "...", "call_id": "__ID__" } },
  { "type": "response.completed", "response": { "id": "__ID__", "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 }, "output": [] } }
]
```

- Cross‑platform tests: Prefer unguarded tests and early‑exit via env checks instead of OS gating unless truly necessary.
```rust
#[tokio::test]
async fn test_apply_patch_context() -> anyhow::Result<()> {
    use codex_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        return Ok(());
    }
    // run_e2e_exec_test(...);
    Ok(())
}
```

**DON’Ts**
- Single‑context assumption: Don’t stop after the first `@@`; handle multiple context lines.
```rust
// Wrong: only considers a single context line
let change_context = lines[0].strip_prefix(CHANGE_CONTEXT_MARKER)
    .map(|s| s.to_string());
```

- Unanchored additions: Don’t append pure additions to EOF when a context anchor exists and the hunk isn’t EOF.
```rust
// Wrong: ignores matched context when not EOF
let insertion_idx = original_lines.len();
```

- Vague errors: Don’t omit which context failed or the path.
```rust
// Wrong: not actionable
return Err(err("Failed to find context"));
```

- Off‑by‑one line numbers: Don’t report errors relative to the first line of the hunk after consuming context markers.
```rust
// Wrong: loses offset from consumed @@ lines
line_number: line_number + 1,
```

- Unnecessary Windows gating: Don’t `#[cfg(not(target_os = "windows"))]` tests that aren’t OS‑specific; prefer capability/env checks.
```rust
// Wrong: hides a portable test for no reason
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_apply_patch_context() { /* ... */ }
```

- Missing EOF newline: Don’t check in fixtures without a final newline.
```json
{ "type": "response.completed" }[] // Wrong: no trailing newline and malformed
```

- Ignoring chunk order: Don’t assume chunk contexts can search backwards or overlap arbitrarily.
```rust
// Wrong: resets search to 0; may match earlier occurrences
line_index = 0; // Do not do this between chunks
```