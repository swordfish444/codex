**DOs**
- Bold the core idea: Gate debug-only commands with `#[cfg(debug_assertions)]` everywhere they’re referenced.
```rust
// enum definition
enum SlashCommand {
    Diff,
    Mention,
    Status,
    Mcp,
    Quit,
    #[cfg(debug_assertions)]
    TestApproval,
}

// usage
impl SlashCommand {
    fn is_public(&self) -> bool {
        match self {
            SlashCommand::Diff
            | SlashCommand::Mention
            | SlashCommand::Status
            | SlashCommand::Mcp
            | SlashCommand::Quit => true,

            #[cfg(debug_assertions)]
            SlashCommand::TestApproval => true,
        }
    }
}
```

- Bold the core idea: Keep user-facing commands (e.g., `Diff`) enabled in release builds.
```rust
// Good: `Diff` is always available; no cfg guard
SlashCommand::Diff
| SlashCommand::Mention
| SlashCommand::Status
| SlashCommand::Mcp
| SlashCommand::Quit => true,
```

- Bold the core idea: Match cfg attributes on arms with the enum’s cfg to avoid missing-variant errors in release.
```rust
// Variant is debug-only…
#[cfg(debug_assertions)]
SlashCommand::TestApproval => true;
// …so there is no arm for it when not(debug_assertions)
```

- Bold the core idea: If a variant exists in all builds but behavior differs, provide both arms so matches stay exhaustive.
```rust
#[cfg(debug_assertions)]
SlashCommand::TestApproval => true,
#[cfg(not(debug_assertions))]
SlashCommand::TestApproval => false,
```

- Bold the core idea: Validate both debug and release to catch cfg mistakes early.
```sh
# Debug (default)
cargo build -p codex-tui
cargo test -p codex-tui

# Release (CI parity)
cargo build --release -p codex-tui
```

**DON’Ts**
- Bold the core idea: Don’t disable the wrong command; don’t gate `Diff` to “fix” release—gate the test-only command.
```rust
// Bad: gating a user command
#[cfg(debug_assertions)]
SlashCommand::Diff => true;
```

- Bold the core idea: Don’t reference a debug-only variant without guarding it.
```rust
// Bad: breaks in release if `TestApproval` is not compiled in
SlashCommand::TestApproval => true;

// Good
#[cfg(debug_assertions)]
SlashCommand::TestApproval => true,
```

- Bold the core idea: Don’t leave variants unmatched under some cfg; ensure exhaustiveness in all configurations.
```rust
// Bad: `TestApproval` removed from enum in release, but arm remains
SlashCommand::TestApproval => true; // compile error in release
```

- Bold the core idea: Don’t duplicate the same variant across arms without cfg separation.
```rust
// Bad: conflicting arms (unreachable/ambiguous)
SlashCommand::Diff => true,
SlashCommand::Diff => false,

// Good: use cfg to separate build-specific behavior
#[cfg(debug_assertions)]
SlashCommand::TestApproval => true,
#[cfg(not(debug_assertions))]
SlashCommand::TestApproval => false,
```