**DOs**
- Bold: Use VS Code rust-analyzer + rustfmt settings aligned with the repo.
```json
// .vscode/settings.json
{
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.extraArgs": ["--all-features", "--tests"],
  "rust-analyzer.rustfmt.extraArgs": ["--config", "imports_granularity=Item"],
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

- Bold: Enforce item-level import formatting (what the rustfmt arg does).
```rust
// Before (collapsed)
use std::collections::{HashMap, HashSet};

// After (Item granularity)
use std::collections::HashMap;
use std::collections::HashSet;
```

- Bold: Use `just` tasks for formatting and lint fixes.
```bash
# From codex-rs/
just fmt
just fix -p codex-tui    # or the crate you changed
```

- Bold: Run tests to verify changes.
```bash
# Test the changed crate first
cargo test -p codex-tui

# If you touched common/core/protocol, run the full suite
cargo test --all-features
```

- Bold: End files with a trailing newline.
```diff
 // Good: editor adds a final newline automatically
 {
   "some": "json"
 }
+
```

**DON’Ts**
- Bold: Don’t switch instructions or tooling to `make`—stick with the `justfile`.
```md
// Bad (don’t add to AGENTS.md)
After changes, run `make format` and `make fix`.

// Good
After changes, run `just fmt` and `just fix -p <project>`.
```

- Bold: Don’t omit the rustfmt extra arg—imports won’t match project style.
```json
// Bad: missing rust-analyzer.rustfmt.extraArgs
{
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.extraArgs": ["--all-features", "--tests"]
}
```

- Bold: Don’t leave files without a final newline.
```diff
 {
   "some": "json"
-}
\ No newline at end of file
+}
```

- Bold: Don’t rely on default import grouping when the repo specifies Item granularity.
```rust
// Bad
use crate::{api, config, ui};

// Good
use crate::api;
use crate::config;
use crate::ui;
```