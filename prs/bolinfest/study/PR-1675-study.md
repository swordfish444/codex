**DOs**
- Align UI text with config: Use a single source of truth by deriving `Display` and `Serialize/Deserialize` on the enum with kebab-case.
```rust
use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AskForApproval {
    #[default]
    #[serde(rename = "untrusted")]
    #[strum(serialize = "untrusted")]
    UnlessTrusted,
    OnFailure,
    Never,
}
```

- Render policies via `Display`: Show the kebab-case value with `to_string()` (or `format!("{}", ...)`) in summaries and TUI.
```rust
("approval", config.approval_policy.to_string())
// or
("approval", format!("{}", config.approval_policy))
```

- Override per-variant when needed: Explicitly rename variants whose desired label isn’t the kebab-case of the Rust identifier.
```rust
#[serde(rename = "untrusted")]
#[strum(serialize = "untrusted")]
UnlessTrusted,
```

- Keep changes centralized: Put naming logic on the enum, not scattered across UI layers.
```rust
// Good: UI just uses Display everywhere
let approval = config.approval_policy.to_string();
```

- Verify and update snapshots when UI text changes: Regenerate and accept TUI snapshots that include the new strings.
```sh
cargo test -p codex-tui
cargo insta pending-snapshots -p codex-tui
cargo insta accept -p codex-tui
```

**DON'Ts**
- Don’t use Debug for user-facing strings: `{:?}` exposes Rust variant names that don’t match config values.
```rust
// Bad: prints "UnlessTrusted"
("approval", format!("{:?}", config.approval_policy))
```

- Don’t use `serde_json::to_string` for display: It adds quotes/JSON formatting and increases complexity.
```rust
// Bad: results in "\"untrusted\""
("approval", serde_json::to_string(&config.approval_policy).unwrap())
```

- Don’t duplicate mapping in the UI: Avoid hand-written matches or ad-hoc conversions in multiple files.
```rust
// Bad: scattered, easy to drift
let label = match config.approval_policy {
    AskForApproval::UnlessTrusted => "untrusted",
    AskForApproval::OnFailure => "on-failure",
    AskForApproval::Never => "never",
};
```

- Don’t rely on implicit case conversion to match external spec: Handle exceptions like `UnlessTrusted` → `untrusted` explicitly.
```rust
// Bad: assumes kebab-case of identifier is correct
#[strum(serialize_all = "kebab-case")] // without per-variant override
// "UnlessTrusted" would become "unless-trusted" (wrong)
```

- Don’t leak internal names into UX: Users should see config-aligned values, not Rust identifiers.
```rust
// Bad UX: shows "OnFailure" instead of "on-failure"
format!("{:?}", AskForApproval::OnFailure);
```