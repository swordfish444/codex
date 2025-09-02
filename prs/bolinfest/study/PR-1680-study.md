**DOs**
- **Guard With Tests**: Add a unit test that asserts the exact guidance line in `prompt.md` exists; fail fast if it drifts.
- **Match Literals Exactly**: Use raw strings and include any literal backslashes before backticks to mirror `prompt.md` precisely.
- **Detect Once**: Cache ripgrep availability with `once_cell::sync::Lazy` so the check runs only once per process.
- **Replace Precisely**: Replace the full guidance line only; return `Cow::Borrowed` when no change is needed to avoid allocations.
- **Assert Replacement**: Add a `debug_assert!` that the replacement actually modified the text to catch mismatches during development.

```rust
// Test: ensure the prompt contains the exact rg guidance line.
#[cfg(test)]
mod tests {
    const BASE: &str = include_str!("../prompt.md");
    const RG_LINE: &str = r"- Do not use \`ls -R\`, \`find\`, or \`grep\` - these are slow in large repos. Use \`rg\` and \`rg --files\`.";

    #[test]
    fn prompt_contains_exact_rg_line() {
        assert!(BASE.contains(RG_LINE), "Update RG_LINE to match prompt.md exactly.");
    }
}
```

```rust
// Robust literals: mirror prompt.md exactly (including escaped backticks if present).
const RG_LINE: &str = r"- Do not use \`ls -R\`, \`find\`, or \`grep\` - these are slow in large repos. Use \`rg\` and \`rg --files\`.";
const RG_LINE_NO_RG: &str = r"- Do not use \`ls -R\`, \`find\`, or \`grep\` - these are slow in large repos.";
```

```rust
// Detect once: cache rg availability.
use once_cell::sync::Lazy;
use std::process::{Command, Stdio};

static RG_AVAILABLE: Lazy<bool> = Lazy::new(|| {
    Command::new("rg")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
});
```

```rust
// Precise replacement with zero-cost borrow when no change is needed.
use std::borrow::Cow;

const BASE_INSTRUCTIONS: &str = include_str!("../prompt.md");

fn base_instructions() -> Cow<'static, str> {
    if *RG_AVAILABLE {
        Cow::Borrowed(BASE_INSTRUCTIONS)
    } else {
        let replaced = BASE_INSTRUCTIONS.replace(RG_LINE, RG_LINE_NO_RG);
        debug_assert!(replaced != BASE_INSTRUCTIONS, "RG_LINE not found in prompt.md; update constants.");
        Cow::Owned(replaced)
    }
}
```

**DON’Ts**
- **Don’t Recommend Missing Tools**: Avoid suggesting `rg` when it isn’t installed.
- **Don’t Trust Assumptions**: Don’t assume backticks/escaping in `prompt.md`; verify the literal string with a test.
- **Don’t Recompute**: Don’t spawn `rg --version` on every call; it’s slow and wasteful.
- **Don’t Replace Loosely**: Don’t replace partial substrings like `"rg"` globally; match and replace the full guidance line only.
- **Don’t Fail Silently**: Don’t perform a replacement without asserting it happened; you’ll miss drift.

```rust
// Don’t: mismatched literal (no escaped backticks) — replacement won’t match prompt.md.
const RG_LINE: &str =
    "- Do not use `ls -R`, `find`, or `grep` - these are slow in large repos. Use `rg` and `rg --files`.";
```

```rust
// Don’t: re-check availability on every call — expensive and unnecessary.
fn base_instructions_bad() -> String {
    let rg_ok = std::process::Command::new("rg")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false); // Recomputed each call — avoid.
    // ...
    String::new()
}
```

```rust
// Don’t: silent no-op replacement — add a check or test.
let replaced = BASE_INSTRUCTIONS.replace(RG_LINE, RG_LINE_NO_RG);
// Missing assertion means drift goes unnoticed.